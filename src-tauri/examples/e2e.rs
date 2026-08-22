//! Uji end-to-end jalur audio (gerbang TASKS P1 + P3).
//!
//! Plays a WAV file to a loopback *output* device while recording from the matching *input*
//! device, then runs the whole pipeline (resample → VAD → segmenter → Whisper) and prints the
//! text. It does not change any system audio setting.
//!
//!     cargo run --release --bin e2e -- sample-en.wav

use auto_transcript_lib::audio::capture::{self, CaptureSink, TrackConfig};
use auto_transcript_lib::audio::resample::ToTarget;
use auto_transcript_lib::audio::writer::read_wav_mono_f32;
use auto_transcript_lib::audio::{devices, looks_like_loopback, Track, Utterance, TARGET_RATE};
use auto_transcript_lib::stt::engine::{TranscribeOpts, WhisperEngine};
use auto_transcript_lib::stt::filter;
use auto_transcript_lib::stt::model_manager;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct Collector {
    utterances: Mutex<Vec<Utterance>>,
    peak_db: Mutex<f32>,
    errors: Mutex<Vec<String>>,
}

impl CaptureSink for Collector {
    fn utterance(&self, u: Utterance) {
        eprintln!(
            "  utterance {:>6} ms .. {:>6} ms ({} sampel)",
            u.start_ms,
            u.end_ms,
            u.pcm.len()
        );
        self.utterances.lock().unwrap().push(u);
    }
    fn partial(&self, _t: Track, _s: i64, _p: Vec<f32>) {}
    fn level(&self, _t: Track, db: f32) {
        let mut p = self.peak_db.lock().unwrap();
        if db > *p {
            *p = db;
        }
    }
    fn silent(&self, _t: Track, secs: u64) {
        eprintln!("  [!] silent for {secs} s");
    }
    fn error(&self, _t: Track, msg: String) {
        eprintln!("  [!] {msg}");
        self.errors.lock().unwrap().push(msg);
    }
}

/// Runs the pipeline (resample → VAD → segmenter → Whisper) straight from a file, without
/// touching any audio device. Used where microphone permission is not available.
fn run_offline(pcm16k: &[f32]) -> i32 {
    use auto_transcript_lib::audio::segmenter::Segmenter;
    let mut seg = Segmenter::new(Track::System);
    let mut utterances = Vec::new();
    for chunk in pcm16k.chunks(TARGET_RATE as usize / 5) {
        seg.push(chunk, &mut utterances);
    }
    seg.flush(&mut utterances);
    eprintln!("Utterances        : {}", utterances.len());
    for u in &utterances {
        eprintln!("  {:>6} ms .. {:>6} ms", u.start_ms, u.end_ms);
    }
    if utterances.is_empty() {
        eprintln!("FAILED: the segmenter found no speech at all");
        return 1;
    }

    let model = "large-v3-turbo-q5_0";
    if !model_manager::is_downloaded(model) {
        eprintln!("model {model} is not downloaded, skipping transcription");
        return 0;
    }
    let engine = WhisperEngine::load(model).expect("load model");
    let mut all = String::new();
    for u in &utterances {
        let out = engine
            .transcribe(
                &u.pcm,
                &TranscribeOpts {
                    lang: None,
                    initial_prompt: None,
                    n_threads: 4,
                    single_segment: false,
                    audio_ctx: 0,
                },
            )
            .expect("transcribe");
        for s in out.segments {
            if let Some(t) = filter::accept(&s.text, s.no_speech) {
                all.push_str(t.trim());
                all.push(' ');
            }
        }
    }
    println!("\nTranskrip:\n  {}", all.trim());
    if all.trim().is_empty() {
        eprintln!("FAILED: the transcript is empty");
        return 1;
    }
    println!("\nPASSED (offline mode): resample → VAD → segmenter → Whisper works.");
    0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let offline = args.iter().any(|a| a == "--offline");
    let path = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .expect("usage: e2e [--offline] <file.wav>");
    let pcm16k = read_wav_mono_f32(std::path::Path::new(&path)).expect("read wav");
    let audio_secs = pcm16k.len() as f32 / TARGET_RATE as f32;
    eprintln!("Source: {path} ({audio_secs:.1} s)");

    if offline {
        std::process::exit(run_offline(&pcm16k));
    }

    // --- perangkat loopback ---
    let sources = devices::list_sources().expect("list devices");
    let input = sources
        .iter()
        .find(|s| s.kind.captures_system_audio())
        .expect("no system audio source");
    eprintln!("Input : {} @ {} Hz", input.name, input.sample_rate);

    let host = cpal::default_host();
    // A virtual loopback device (BlackHole and friends) is used when one exists, because
    // then it is what the input side is tapping. Otherwise the plain default output is
    // exactly right: with a CoreAudio process tap or WASAPI loopback, the device being
    // captured IS an ordinary output device. Insisting on a virtual one made this test
    // impossible to run on Windows, where no such device exists.
    let out_device = host
        .output_devices()
        .expect("list output devices")
        .find(|d| {
            d.description()
                .map(|x| looks_like_loopback(x.name()))
                .unwrap_or(false)
        })
        .or_else(|| host.default_output_device())
        .expect("no output device to play through");
    let out_cfg = out_device.default_output_config().expect("config output");
    let out_rate = out_cfg.sample_rate();
    let out_ch = out_cfg.channels() as usize;
    eprintln!(
        "Output: {} @ {out_rate} Hz, {out_ch} kanal",
        out_device.description().map(|d| d.name().to_string()).unwrap_or_default()
    );

    // --- prepare the samples for playback: 16 kHz → device rate, then interleave ---
    let mut up = ToTarget::new(TARGET_RATE, out_rate).expect("upsampler");
    let mut mono = Vec::with_capacity(pcm16k.len() * 4);
    up.push(&pcm16k, &mut mono).expect("upsample");
    let mut interleaved = Vec::with_capacity(mono.len() * out_ch);
    for s in &mono {
        for _ in 0..out_ch {
            interleaved.push(*s);
        }
    }
    let total_frames = mono.len();
    let playback = Arc::new(interleaved);
    let cursor = Arc::new(AtomicUsize::new(0));

    // --- mulai merekam dulu, baru memutar ---
    eprintln!("Preparing capture…");
    let sink = Arc::new(Collector::default());
    let wav_out = std::env::temp_dir().join("e2e-capture.wav");
    let handle = capture::start(
        vec![TrackConfig {
            track: Track::System,
            device_id: input.id.clone(),
            wav_path: wav_out.clone(),
            emit_partials: false,
            segment: true,
        }],
        sink.clone() as Arc<dyn CaptureSink>,
    )
    .expect("start capture");
    eprintln!("Capture running.");
    std::thread::sleep(Duration::from_millis(600));

    let pb = playback.clone();
    let cur = cursor.clone();
    let stream = out_device
        .build_output_stream(
            out_cfg.config(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let start = cur.fetch_add(data.len(), Ordering::SeqCst);
                for (i, d) in data.iter_mut().enumerate() {
                    *d = pb.get(start + i).copied().unwrap_or(0.0);
                }
            },
            |e| eprintln!("output error: {e}"),
            None,
        )
        .expect("build output stream");
    stream.play().expect("play");
    eprintln!("Playing…");

    let play_secs = total_frames as f32 / out_rate as f32;
    std::thread::sleep(Duration::from_secs_f32(play_secs + 1.2));
    drop(stream);
    std::thread::sleep(Duration::from_millis(400));
    handle.stop();

    let peak = *sink.peak_db.lock().unwrap();
    let utterances = std::mem::take(&mut *sink.utterances.lock().unwrap());
    let errors = sink.errors.lock().unwrap().clone();

    println!("\n--- Result ---");
    println!("Peak level        : {peak:.1} dBFS");
    println!("Utterances        : {}", utterances.len());
    println!("Recording file    : {}", wav_out.display());
    if !errors.is_empty() {
        println!("Errors            : {errors:?}");
    }

    let mut fail = Vec::new();
    if peak < -50.0 {
        fail.push(format!("level too low ({peak:.1} dBFS) — audio never arrived"));
    }
    if utterances.is_empty() {
        fail.push("no utterances were detected".into());
    }

    // --- transkrip hasil rekaman ---
    let model = "large-v3-turbo-q5_0";
    if model_manager::is_downloaded(model) && !utterances.is_empty() {
        let engine = WhisperEngine::load(model).expect("load model");
        let mut all = String::new();
        for u in &utterances {
            let out = engine
                .transcribe(
                    &u.pcm,
                    &TranscribeOpts {
                        lang: None,
                        initial_prompt: None,
                        n_threads: 4,
                        single_segment: false,
                    audio_ctx: 0,
                    },
                )
                .expect("transcribe");
            for s in out.segments {
                if let Some(t) = filter::accept(&s.text, s.no_speech) {
                    all.push_str(t.trim());
                    all.push(' ');
                }
            }
        }
        println!("\nTranscript of the recording:\n  {}", all.trim());
        if all.trim().is_empty() {
            fail.push("the transcript is empty".into());
        }
    }

    if fail.is_empty() {
        println!("\nPASSED: output audio → loopback → capture → VAD → Whisper works.");
    } else {
        println!("\nFAILED:");
        for f in &fail {
            println!("  - {f}");
        }
        std::process::exit(1);
    }
}
