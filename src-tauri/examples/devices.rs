//! Lists the audio sources that were detected, then taps an output device for a few seconds
//! to prove that direct capture actually works.
//!
//!     cargo run --release --example devices -- [seconds]

use auto_transcript_lib::audio::capture::{self, CaptureSink, TrackConfig};
use auto_transcript_lib::audio::{devices, SourceKind, Track, Utterance};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Probe {
    peak_db: Mutex<f32>,
    frames: AtomicUsize,
    errors: Mutex<Vec<String>>,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            // Not 0.0: dBFS is always negative, so a starting value of 0.0 would never be
            // exceeded and the peak would always read as 0.0.
            peak_db: Mutex::new(-120.0),
            frames: AtomicUsize::new(0),
            errors: Mutex::new(Vec::new()),
        }
    }
}

impl CaptureSink for Probe {
    fn utterance(&self, _u: Utterance) {}
    fn partial(&self, _t: Track, _s: i64, _p: Vec<f32>) {}
    fn level(&self, _t: Track, db: f32) {
        self.frames.fetch_add(1, Ordering::SeqCst);
        let mut p = self.peak_db.lock().unwrap();
        if db > *p {
            *p = db;
        }
    }
    fn silent(&self, _t: Track, _s: u64) {}
    fn error(&self, _t: Track, msg: String) {
        self.errors.lock().unwrap().push(msg);
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("auto_transcript_lib=debug")
        .with_writer(std::io::stderr)
        .init();
    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(6);

    let sources = devices::list_sources().expect("list sources");
    println!("{:<14} {:<34} {:>7} {:>4}  DEFAULT", "KIND", "NAME", "RATE", "CH");
    for s in &sources {
        println!(
            "{:<14} {:<34} {:>7} {:>4}  {}",
            format!("{:?}", s.kind),
            s.name.chars().take(34).collect::<String>(),
            s.sample_rate,
            s.channels,
            if s.is_default { "yes" } else { "" }
        );
    }

    let (sys, mic) = devices::guess_defaults(&sources);
    println!("\nAutomatic choice → system: {sys:?}\n                   mic   : {mic:?}");

    let want = std::env::args().nth(2);
    let Some(target) = want
        .as_ref()
        .and_then(|w| sources.iter().find(|s| s.name.contains(w.as_str())))
        .or_else(|| sources.iter().find(|s| s.kind == SourceKind::SystemOutput))
        .or_else(|| sources.iter().find(|s| s.kind == SourceKind::VirtualLoopback))
    else {
        println!("\nNo system audio source available to test.");
        return;
    };

    println!("\nTapping '{}' ({:?}) for {secs} seconds…", target.name, target.kind);
    println!("Play any audio now to watch the level rise.");

    let probe = Arc::new(Probe::default());
    let wav = std::env::temp_dir().join("devices-probe.wav");
    let handle = capture::start(
        vec![TrackConfig {
            track: Track::System,
            device_id: target.id.clone(),
            wav_path: wav.clone(),
            emit_partials: false,
            segment: false,
        }],
        probe.clone() as Arc<dyn CaptureSink>,
    )
    .expect("start capture");

    std::thread::sleep(Duration::from_millis(300));
    for _ in 0..4 {
        let _ = std::process::Command::new("afplay")
            .arg("/System/Library/Sounds/Glass.aiff")
            .status();
    }
    std::thread::sleep(Duration::from_secs(secs));
    handle.stop();

    let peak = *probe.peak_db.lock().unwrap();
    let frames = probe.frames.load(Ordering::SeqCst);
    let errors = probe.errors.lock().unwrap().clone();
    println!("\nLevel frames  : {frames}");
    println!("Peak level    : {peak:.1} dBFS");
    if !errors.is_empty() {
        println!("Errors        : {errors:?}");
    }
    println!("File          : {}", wav.display());
    if frames == 0 {
        println!("\nFAILED: the audio stream never ran.");
    } else if peak < -70.0 {
        println!("\nThe stream ran but was completely silent — capture permission was probably denied.");
    } else {
        println!("\nPASSED: system audio captured with no extra driver.");
    }
}
