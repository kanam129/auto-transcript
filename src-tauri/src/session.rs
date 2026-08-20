use crate::audio::capture::{self, CaptureHandle, CaptureSink, TrackConfig};
use crate::audio::{devices, Track, Utterance};
use crate::error::{AppError, Result};
use crate::events;
use crate::paths;
use crate::settings::Settings;
use crate::stt::worker::{FinalJob, PartialJob, TranscriptSink, Transcribers, WorkerConfig};
use crate::store::db::{Db, NewSegment, NewSession, SessionMeta};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// How long to wait for the transcription queue to drain when a session stops.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
pub struct RecordingState {
    pub recording: bool,
    pub session_id: Option<String>,
    pub title: Option<String>,
    pub elapsed_ms: i64,
    pub model_id: Option<String>,
    pub source_name: Option<String>,
    pub queue_depth: usize,
}

pub struct RecordingSession {
    pub id: String,
    pub title: String,
    pub started_at: i64,
    pub start_instant: Instant,
    pub model_id: String,
    pub source_name: String,
    capture: Option<CaptureHandle>,
    workers: Arc<Transcribers>,
    _sink: Arc<SessionSink>,
}

impl RecordingSession {
    pub fn state(&self) -> RecordingState {
        RecordingState {
            recording: true,
            session_id: Some(self.id.clone()),
            title: Some(self.title.clone()),
            elapsed_ms: self.start_instant.elapsed().as_millis() as i64,
            model_id: Some(self.model_id.clone()),
            source_name: Some(self.source_name.clone()),
            queue_depth: self.workers.queue_depth(),
        }
    }
}

/// Where every audio and transcription thread ends up: writes to the database and forwards
/// whatever the user needs to see to the frontend.
///
/// The microphone track is transcribed and stored — material for AI summaries — but never
/// emitted as an event. The transcript window shows system output only.
pub struct SessionSink {
    app: AppHandle,
    db: Arc<Db>,
    session_id: String,
    workers: OnceLock<Arc<Transcribers>>,
}

impl SessionSink {
    fn set_workers(&self, w: Arc<Transcribers>) {
        let _ = self.workers.set(w);
    }
}

impl CaptureSink for SessionSink {
    fn utterance(&self, u: Utterance) {
        if let Some(w) = self.workers.get() {
            w.submit_final(FinalJob {
                track: u.track,
                start_ms: u.start_ms,
                end_ms: u.end_ms,
                pcm: u.pcm,
            });
        }
    }

    fn partial(&self, _track: Track, start_ms: i64, pcm: Vec<f32>) {
        if let Some(w) = self.workers.get() {
            w.submit_partial(PartialJob { start_ms, pcm });
        }
    }

    fn level(&self, track: Track, db: f32) {
        events::emit(
            &self.app,
            events::AUDIO_LEVEL,
            events::LevelEvent {
                track: track.as_str(),
                db,
            },
        );
    }

    fn silent(&self, track: Track, secs: u64) {
        tracing::warn!("track {} has been silent for {secs} s", track.as_str());
        events::emit(
            &self.app,
            events::AUDIO_SILENT,
            events::SilentEvent {
                track: track.as_str(),
                seconds: secs,
            },
        );
    }

    fn error(&self, track: Track, msg: String) {
        tracing::error!("audio {}: {msg}", track.as_str());
        events::emit(
            &self.app,
            events::AUDIO_ERROR,
            events::ErrorEvent {
                code: "audio",
                message: msg,
            },
        );
    }
}

impl TranscriptSink for SessionSink {
    fn final_segment(
        &self,
        track: Track,
        start_ms: i64,
        end_ms: i64,
        text: String,
        lang: String,
        confidence: f32,
    ) {
        let seg = NewSegment {
            session_id: &self.session_id,
            track,
            start_ms,
            end_ms,
            text: &text,
            lang: Some(&lang),
            confidence: Some(confidence),
        };
        let id = match self.db.insert_segment(&seg) {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("could not store segment: {e}");
                return;
            }
        };
        if track == Track::System {
            events::emit(
                &self.app,
                events::TRANSCRIPT_SEGMENT,
                events::SegmentEvent {
                    session_id: self.session_id.clone(),
                    id,
                    start_ms,
                    end_ms,
                    text,
                    lang: Some(lang),
                    confidence: Some(confidence),
                },
            );
        }
    }

    fn partial(&self, start_ms: i64, text: String) {
        events::emit(
            &self.app,
            events::TRANSCRIPT_PARTIAL,
            events::PartialEvent {
                session_id: self.session_id.clone(),
                start_ms,
                text,
            },
        );
    }

    fn degraded(&self, from: String, to: String, reason: String) {
        tracing::warn!("model downgraded from {from} to {to}: {reason}");
        events::emit(
            &self.app,
            events::ENGINE_DEGRADED,
            events::DegradedEvent { from, to, reason },
        );
    }

    fn error(&self, msg: String) {
        events::emit(
            &self.app,
            events::AUDIO_ERROR,
            events::ErrorEvent {
                code: "stt",
                message: msg,
            },
        );
    }
}

/// Shown when there is no system audio source at all. The wording differs per platform
/// because the way out genuinely differs.
fn no_system_source_hint() -> String {
    #[cfg(target_os = "windows")]
    {
        "No output device available to capture. Make sure speakers or headphones are active \
         in Windows Sound settings."
            .to_string()
    }
    #[cfg(target_os = "macos")]
    {
        "No system audio source found. Pick an output device in Settings, or install \
         BlackHole as a fallback."
            .to_string()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        "No system audio source is available.".to_string()
    }
}

pub fn default_title(started_at: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_millis_opt(started_at).single() {
        Some(dt) => format!("Meeting {}", dt.format("%d %b %Y, %H:%M")),
        None => "Meeting".to_string(),
    }
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub struct StartOptions {
    pub title: Option<String>,
    pub system_source_id: Option<String>,
    pub mic_source_id: Option<String>,
}

pub fn start(
    app: AppHandle,
    db: Arc<Db>,
    settings: &Settings,
    opts: StartOptions,
) -> Result<RecordingSession> {
    let sources = devices::list_sources()?;
    let (guess_sys, guess_mic) = devices::guess_defaults(&sources);

    let system_id = opts
        .system_source_id
        .or_else(|| settings.system_source_id.clone())
        .or(guess_sys)
        .ok_or_else(|| AppError::Audio(no_system_source_hint()))?;
    // Confirm the device exists right now, rather than being a leftover in old settings.
    devices::find_device(&system_id)?;

    let mic_id = opts
        .mic_source_id
        .or_else(|| settings.mic_source_id.clone())
        .or(guess_mic)
        .filter(|id| devices::find_device(id).is_ok());

    if !crate::stt::model_manager::is_downloaded(&settings.model_id) {
        return Err(AppError::Model(format!(
            "model '{}' has not been downloaded",
            settings.model_id
        )));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let started_at = now_ms();
    let title = opts
        .title
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| default_title(started_at));

    let dir = paths::session_dir(&id)?;
    let system_wav: PathBuf = dir.join("system.wav");
    let mic_wav: Option<PathBuf> = mic_id.as_ref().map(|_| dir.join("mic.wav"));

    let source_name = devices::device_label(&system_id);
    db.create_session(&NewSession {
        id: &id,
        title: &title,
        started_at,
        model_used: &settings.model_id,
        source_name: &source_name,
        lang_mode: match settings.lang_mode {
            crate::settings::LangMode::Auto => "auto",
            crate::settings::LangMode::En => "en",
            crate::settings::LangMode::Id => "id",
        },
        system_wav: system_wav.to_str(),
        mic_wav: mic_wav.as_ref().and_then(|p| p.to_str()),
    })?;

    let sink = Arc::new(SessionSink {
        app: app.clone(),
        db: db.clone(),
        session_id: id.clone(),
        workers: OnceLock::new(),
    });

    let worker_cfg = WorkerConfig {
        model_id: settings.model_id.clone(),
        fallback_model_id: settings.partial_model_id.clone(),
        partial_model_id: settings.partial_model_id.clone(),
        partials_enabled: settings.partials_enabled,
        lang_mode: settings.lang_mode,
        vocabulary: settings.vocabulary.clone(),
        n_threads: 4,
    };
    let workers = Arc::new(Transcribers::start(
        worker_cfg,
        sink.clone() as Arc<dyn TranscriptSink>,
    )?);
    sink.set_workers(workers.clone());

    let mut tracks = vec![TrackConfig {
        track: Track::System,
        device_id: system_id.clone(),
        wav_path: system_wav,
        emit_partials: settings.partials_enabled,
        segment: true,
    }];
    if let (Some(mic), Some(path)) = (mic_id.clone(), mic_wav.clone()) {
        tracks.push(TrackConfig {
            track: Track::Mic,
            device_id: mic,
            wav_path: path,
            emit_partials: false,
            // The microphone is segmented and transcribed too, so AI summaries can see both
            // sides of a conversation — but its output never reaches the transcript window.
            segment: settings.mic_transcribe,
        });
    }

    let capture = match capture::start(tracks, sink.clone() as Arc<dyn CaptureSink>) {
        Ok(c) => c,
        Err(e) => {
            workers.shutdown(Duration::from_secs(1));
            let _ = db.delete_session(&id);
            return Err(e);
        }
    };

    tracing::info!(
        "session {id} started (source '{source_name}', model '{}')",
        settings.model_id
    );

    Ok(RecordingSession {
        id,
        title,
        started_at,
        start_instant: Instant::now(),
        model_id: settings.model_id.clone(),
        source_name,
        capture: Some(capture),
        workers,
        _sink: sink,
    })
}

/// Stops a session in order: stop capture (the pipeline threads flush the remaining audio and
/// the final sentence), wait for the transcription queue to drain, then close the session.
pub fn stop(mut session: RecordingSession, db: &Db) -> Result<SessionMeta> {
    if let Some(c) = session.capture.take() {
        c.stop();
    }
    session.workers.shutdown(DRAIN_TIMEOUT);

    let ended_at = now_ms();
    let duration = session.start_instant.elapsed().as_millis() as i64;
    db.finish_session(&session.id, ended_at, duration)?;
    tracing::info!("session {} finished ({} ms)", session.id, duration);
    db.get_session(&session.id)
}
