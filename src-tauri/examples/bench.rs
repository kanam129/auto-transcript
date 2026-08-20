//! Harness benchmark model (TASKS P2-T20).
//!
//! Menjalankan tiap model terhadap berkas WAV 16 kHz mono dan melaporkan RTF
//! (real-time factor) serta puncak pemakaian memori, lalu mencetak tabel Markdown.
//!
//!     cargo run --release --bin bench -- sample-en.wav sample-id.wav

use auto_transcript_lib::audio::writer::read_wav_mono_f32;
use auto_transcript_lib::audio::TARGET_RATE;
use auto_transcript_lib::stt::engine::{TranscribeOpts, WhisperEngine};
use auto_transcript_lib::stt::model_manager;
use std::path::PathBuf;
use std::time::Instant;

fn rss_mb() -> f64 {
    let pid = std::process::id();
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|kb| kb / 1024.0)
        .unwrap_or(0.0)
}

struct Row {
    model: String,
    sample: String,
    audio_secs: f32,
    load_secs: f64,
    infer_secs: f64,
    rtf: f64,
    rss_mb: f64,
    lang: String,
    text: String,
}

fn main() {
    let files: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if files.is_empty() {
        eprintln!("usage: bench <file.wav> [file.wav ...]");
        std::process::exit(2);
    }

    let mut rows: Vec<Row> = Vec::new();

    for spec in model_manager::catalog() {
        if !model_manager::is_downloaded(spec.id) {
            eprintln!("skipping {} (not downloaded)", spec.id);
            continue;
        }
        eprintln!("== loading {}", spec.id);
        let t = Instant::now();
        let engine = match WhisperEngine::load(spec.id) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("   failed: {e}");
                continue;
            }
        };
        let load_secs = t.elapsed().as_secs_f64();

        for f in &files {
            let pcm = match read_wav_mono_f32(f) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("   {}: {e}", f.display());
                    continue;
                }
            };
            let audio_secs = pcm.len() as f32 / TARGET_RATE as f32;
            let opts = TranscribeOpts {
                lang: None, // auto — sekaligus menguji deteksi bahasa
                initial_prompt: None,
                n_threads: 4,
                single_segment: false,
                    audio_ctx: 0,
            };
            let t = Instant::now();
            let out = match engine.transcribe(&pcm, &opts) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("   transcription failed: {e}");
                    continue;
                }
            };
            let infer_secs = t.elapsed().as_secs_f64();
            let text = out
                .segments
                .iter()
                .map(|s| s.text.trim())
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!(
                "   {:<28} {:>5.2}s audio → {:>5.2}s  RTF {:.2}  [{}] {}",
                f.file_name().unwrap_or_default().to_string_lossy(),
                audio_secs,
                infer_secs,
                infer_secs / audio_secs as f64,
                out.lang,
                text.chars().take(70).collect::<String>()
            );
            rows.push(Row {
                model: spec.id.to_string(),
                sample: f.file_stem().unwrap_or_default().to_string_lossy().to_string(),
                audio_secs,
                load_secs,
                infer_secs,
                rtf: infer_secs / audio_secs as f64,
                rss_mb: rss_mb(),
                lang: out.lang,
                text,
            });
        }
        drop(engine);
    }

    println!("\n| Model | Sampel | Audio (s) | Load (s) | Inference (s) | RTF | Peak RSS (MB) | Bahasa |");
    println!("|---|---|---:|---:|---:|---:|---:|---|");
    for r in &rows {
        println!(
            "| {} | {} | {:.1} | {:.1} | {:.2} | **{:.2}** | {:.0} | {} |",
            r.model, r.sample, r.audio_secs, r.load_secs, r.infer_secs, r.rtf, r.rss_mb, r.lang
        );
    }
    println!("\n### Text produced\n");
    for r in &rows {
        println!("- **{} / {}** ({}): {}", r.model, r.sample, r.lang, r.text);
    }
}
