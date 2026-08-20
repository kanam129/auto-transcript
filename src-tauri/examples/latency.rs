//! Sweeps `audio_ctx` and language mode over an 8-second chunk to find the best trade-off
//! between speed and quality on the final-text path.

use auto_transcript_lib::audio::writer::read_wav_mono_f32;
use auto_transcript_lib::audio::TARGET_RATE;
use auto_transcript_lib::stt::engine::{TranscribeOpts, WhisperEngine};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let model = std::env::var("MODEL").unwrap_or_else(|_| "large-v3-turbo-q5_0".into());
    let engine = WhisperEngine::load(&model).expect("load model");
    let secs: f32 = std::env::var("SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(8.0);

    println!("model {model}, {secs}-second chunk\n");
    println!("| Sample | Language | audio_ctx | Time | Text |");
    println!("|---|---|---:|---:|---|");

    for path in &args {
        let pcm = read_wav_mono_f32(std::path::Path::new(path)).expect("read wav");
        let n = ((secs * TARGET_RATE as f32) as usize).min(pcm.len());
        let chunk = &pcm[..n];
        let name = std::path::Path::new(path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let forced: &str = if name.contains("-id") { "id" } else { "en" };
        for (label, lang) in [("auto", None), ("forced correct", Some(forced))] {
            for ctx in [0, 1100, 900, 700, 528] {
                let opts = TranscribeOpts {
                    lang,
                    initial_prompt: None,
                    n_threads: 4,
                    single_segment: false,
                    audio_ctx: ctx,
                };
                let _ = engine.transcribe(&chunk[..chunk.len().min(8000)], &opts);
                let t = Instant::now();
                let out = engine.transcribe(chunk, &opts).expect("transcribe");
                let dur = t.elapsed().as_secs_f64();
                let text: String = out
                    .segments
                    .iter()
                    .map(|s| s.text.trim())
                    .collect::<Vec<_>>()
                    .join(" ");
                println!(
                    "| {name} | {label} | {} | **{dur:.2}s** | {} |",
                    if ctx == 0 { "full".into() } else { ctx.to_string() },
                    text.chars().take(85).collect::<String>()
                );
            }
        }
    }
}
