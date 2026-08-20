use crate::audio::segmenter::Segmenter;
use crate::audio::writer::read_wav_mono_f32;
use crate::audio::{Track, Utterance, TARGET_RATE};
use crate::error::Result;
use crate::events;
use crate::settings::Settings;
use crate::stt::engine::{TranscribeOpts, WhisperEngine};
use crate::stt::filter;
use crate::store::db::{Db, NewSegment};
use serde::Serialize;
use std::path::Path;
use tauri::AppHandle;

#[derive(Serialize, Clone)]
struct Progress {
    session_id: String,
    done: usize,
    total: usize,
}

/// Runs the same pipeline as realtime mode, but sourced from a WAV file. With no time
/// pressure, the main model is used as-is and never downgraded.
pub fn run(
    app: &AppHandle,
    db: &Db,
    session_id: &str,
    wav: &Path,
    settings: &Settings,
) -> Result<()> {
    let pcm = read_wav_mono_f32(wav)?;
    tracing::info!(
        "re-transcribing microphone track: {:.1} s of audio from {}",
        pcm.len() as f32 / TARGET_RATE as f32,
        wav.display()
    );

    let mut seg = Segmenter::new(Track::Mic);
    let mut utterances: Vec<Utterance> = Vec::new();
    for chunk in pcm.chunks(TARGET_RATE as usize) {
        seg.push(chunk, &mut utterances);
    }
    seg.flush(&mut utterances);

    let total = utterances.len();
    if total == 0 {
        events::emit(
            app,
            events::MIC_PROGRESS,
            Progress {
                session_id: session_id.into(),
                done: 0,
                total: 0,
            },
        );
        return Ok(());
    }

    let engine = WhisperEngine::load(&settings.model_id)?;
    db.delete_segments(session_id, Track::Mic)?;

    let mut sticky = settings.lang_mode.forced().unwrap_or("en").to_string();
    for (i, u) in utterances.iter().enumerate() {
        let duration = u.end_ms - u.start_ms;
        let lang = match settings.lang_mode.forced() {
            Some(l) => Some(l.to_string()),
            None if duration < 1500 => Some(sticky.clone()),
            None => None,
        };
        let opts = TranscribeOpts {
            lang: lang.as_deref(),
            initial_prompt: Some(&settings.vocabulary),
            n_threads: 4,
            single_segment: false,
                    audio_ctx: 0,
        };
        match engine.transcribe(&u.pcm, &opts) {
            Ok(out) => {
                if out.lang == "en" || out.lang == "id" {
                    sticky = out.lang.clone();
                }
                for s in out.segments {
                    let Some(text) = filter::accept(&s.text, s.no_speech) else {
                        continue;
                    };
                    let start = u.start_ms + s.t0_ms;
                    let end = (u.start_ms + s.t1_ms).min(u.end_ms).max(start);
                    db.insert_segment(&NewSegment {
                        session_id,
                        track: Track::Mic,
                        start_ms: start,
                        end_ms: end,
                        text: &text,
                        lang: Some(&out.lang),
                        confidence: Some(1.0 - s.no_speech),
                    })?;
                }
            }
            Err(e) => tracing::warn!("microphone chunk {i} failed: {e}"),
        }
        events::emit(
            app,
            events::MIC_PROGRESS,
            Progress {
                session_id: session_id.into(),
                done: i + 1,
                total,
            },
        );
    }
    tracing::info!("microphone re-transcription finished: {total} chunks");
    Ok(())
}
