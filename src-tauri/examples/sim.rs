//! Realtime simulation: replays a WAV file into the real pipeline at real speed and measures
//! how long text takes to appear after the audio was heard.
//!
//! This is the number a user feels, not RTF.
//!
//!     cargo run --release --example sim -- sample-en.wav

use auto_transcript_lib::audio::segmenter::Segmenter;
use auto_transcript_lib::audio::writer::read_wav_mono_f32;
use auto_transcript_lib::audio::{Track, Utterance, TARGET_RATE};
use auto_transcript_lib::settings::LangMode;
use auto_transcript_lib::stt::worker::{
    FinalJob, PartialJob, TranscriptSink, Transcribers, WorkerConfig,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Recorder {
    t0: Instant,
    /// How much audio (ms) has been fed in — used to compute the delay.
    fed_ms: AtomicU64,
    events: Mutex<Vec<(String, i64, i64, String)>>, // jenis, latensi ms, posisi ms, teks
}

impl TranscriptSink for Recorder {
    fn final_segment(&self, _t: Track, _s: i64, end_ms: i64, text: String, _l: String, _c: f32) {
        let wall = self.t0.elapsed().as_millis() as i64;
        self.events
            .lock()
            .unwrap()
            .push(("FINAL".into(), wall - end_ms, end_ms, text));
    }
    fn partial(&self, _start_ms: i64, text: String) {
        let wall = self.t0.elapsed().as_millis() as i64;
        let fed = self.fed_ms.load(Ordering::SeqCst) as i64;
        self.events
            .lock()
            .unwrap()
            .push(("preview".into(), wall - fed, fed, text));
    }
    fn degraded(&self, from: String, to: String, why: String) {
        eprintln!("[degraded] {from} → {to}: {why}");
    }
    fn error(&self, msg: String) {
        eprintln!("[error] {msg}");
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("auto_transcript_lib=debug")
        .with_writer(std::io::stderr)
        .init();
    let path = std::env::args().nth(1).expect("usage: sim <file.wav>");
    let pcm = read_wav_mono_f32(std::path::Path::new(&path)).expect("read wav");
    let total_ms = pcm.len() as i64 * 1000 / TARGET_RATE as i64;
    eprintln!("Replaying {path} ({total_ms} ms) at real speed…\n");

    let rec = Arc::new(Recorder {
        t0: Instant::now(),
        fed_ms: AtomicU64::new(0),
        events: Mutex::new(Vec::new()),
    });

    let workers = Transcribers::start(
        WorkerConfig {
            model_id: "large-v3-turbo-q5_0".into(),
            fallback_model_id: "base-q5_1".into(),
            partial_model_id: "small-q5_1".into(),
            partials_enabled: true,
            lang_mode: LangMode::Auto,
            vocabulary: String::new(),
            n_threads: 4,
        },
        rec.clone() as Arc<dyn TranscriptSink>,
    )
    .expect("start workers");

    // Feed 100 ms per step, exactly like the pipeline thread in the app.
    const STEP: usize = TARGET_RATE as usize / 10;
    let mut seg = Segmenter::new(Track::System);
    let mut out: Vec<Utterance> = Vec::new();
    let mut last_partial = Instant::now();
    let start = Instant::now();

    for (i, chunk) in pcm.chunks(STEP).enumerate() {
        let target = start + Duration::from_millis((i as u64 + 1) * 100);
        out.clear();
        seg.push(chunk, &mut out);
        rec.fed_ms
            .store(((i as u64 + 1) * 100).min(total_ms as u64), Ordering::SeqCst);
        for u in out.drain(..) {
            workers.submit_final(FinalJob {
                track: u.track,
                start_ms: u.start_ms,
                end_ms: u.end_ms,
                pcm: u.pcm,
            });
        }
        if last_partial.elapsed() >= Duration::from_millis(700) {
            last_partial = Instant::now();
            if let Some((s, p)) = seg.partial_snapshot(TARGET_RATE as usize / 2) {
                workers.submit_partial(PartialJob {
                    start_ms: s,
                    pcm: p,
                });
            }
        }
        let now = Instant::now();
        if target > now {
            std::thread::sleep(target - now);
        }
    }
    out.clear();
    seg.flush(&mut out);
    for u in out.drain(..) {
        workers.submit_final(FinalJob {
            track: u.track,
            start_ms: u.start_ms,
            end_ms: u.end_ms,
            pcm: u.pcm,
        });
    }
    workers.shutdown(Duration::from_secs(30));

    let events = rec.events.lock().unwrap();
    println!("\n| Kind | Audio position | Delay | Text |");
    println!("|---|---:|---:|---|");
    for (kind, delay, pos, text) in events.iter() {
        println!(
            "| {kind} | {:.1}s | **{delay} ms** | {} |",
            *pos as f32 / 1000.0,
            text.chars().take(72).collect::<String>()
        );
    }
    let finals: Vec<i64> = events
        .iter()
        .filter(|e| e.0 == "FINAL")
        .map(|e| e.1)
        .collect();
    let partials: Vec<i64> = events
        .iter()
        .filter(|e| e.0 != "FINAL")
        .map(|e| e.1)
        .collect();
    let avg = |v: &[i64]| if v.is_empty() { 0 } else { v.iter().sum::<i64>() / v.len() as i64 };
    println!(
        "\nPreview: {} events, mean {} ms · Final: {} segments, mean {} ms, worst {} ms",
        partials.len(),
        avg(&partials),
        finals.len(),
        avg(&finals),
        finals.iter().max().copied().unwrap_or(0)
    );
}
