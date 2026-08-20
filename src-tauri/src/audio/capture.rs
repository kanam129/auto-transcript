use super::devices::find_device;
use super::resample::ToTarget;
use super::segmenter::Segmenter;
use super::writer::WavTrackWriter;
use super::{Track, Utterance, TARGET_RATE};
use crate::error::{AppError, Result};
use cpal::traits::{DeviceTrait, StreamTrait};

fn supports_input(device: &cpal::Device) -> bool {
    device
        .description()
        .map(|d| d.supports_input())
        .unwrap_or(false)
}
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Where the audio pipeline sends its results. Implemented in `session.rs`.
pub trait CaptureSink: Send + Sync + 'static {
    fn utterance(&self, u: Utterance);
    fn partial(&self, track: Track, start_ms: i64, pcm: Vec<f32>);
    fn level(&self, track: Track, db: f32);
    /// Called once when the system track has been completely silent for a while.
    fn silent(&self, track: Track, secs: u64);
    fn error(&self, track: Track, msg: String);
}

pub struct TrackConfig {
    pub track: Track,
    pub device_id: String,
    pub wav_path: PathBuf,
    /// Only the system track produces realtime previews.
    pub emit_partials: bool,
    pub segment: bool,
}

pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl CaptureHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

const LEVEL_INTERVAL: Duration = Duration::from_millis(50);
/// Preview cadence. Tried at 450 ms and moved back to 700 ms after measuring: at 450 ms the
/// preview model was not finished when the next tick arrived, work queued up, and BOTH the
/// preview and the final text got slower.
const PARTIAL_INTERVAL: Duration = Duration::from_millis(700);
const PARTIAL_MIN_SAMPLES: usize = TARGET_RATE as usize / 2; // 0.5 s
const SILENCE_DB: f32 = -60.0;
const SILENCE_WARN_SECS: u64 = 10;

pub fn start(configs: Vec<TrackConfig>, sink: Arc<dyn CaptureSink>) -> Result<CaptureHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let mut threads = Vec::new();

    for cfg in configs {
        let device = find_device(&cfg.device_id)?;
        // Output devices have no "input config". Take the format from the output side and
        // let cpal turn it into an input stream — a process tap on macOS, WASAPI loopback
        // on Windows.
        let supported = if supports_input(&device) {
            device
                .default_input_config()
                .map_err(|e| AppError::Audio(format!("input config for '{}': {e}", cfg.device_id)))?
        } else {
            device
                .default_output_config()
                .map_err(|e| AppError::Audio(format!("output config for '{}': {e}", cfg.device_id)))?
        };
        let input_rate = supported.sample_rate();
        let channels = supported.channels() as usize;

        // A 10-second buffer: long enough to absorb scheduling jitter, short enough that a
        // genuinely stalled consumer shows up quickly.
        let rb = HeapRb::<f32>::new(input_rate as usize * 10);
        let (prod, cons) = rb.split();
        let prod = Arc::new(Mutex::new(prod));

        let dev_stop = stop.clone();
        let dev_sink = sink.clone();
        let dev_id = cfg.device_id.clone();
        let track = cfg.track;
        let prod_for_dev = prod.clone();
        threads.push(
            std::thread::Builder::new()
                .name(format!("audio-dev-{}", track.as_str()))
                .spawn(move || {
                    run_device(track, dev_id, supported, prod_for_dev, dev_stop, dev_sink)
                })
                .map_err(|e| AppError::Audio(format!("could not spawn device thread: {e}")))?,
        );

        let proc_stop = stop.clone();
        let proc_sink = sink.clone();
        threads.push(
            std::thread::Builder::new()
                .name(format!("audio-proc-{}", cfg.track.as_str()))
                .spawn(move || {
                    if let Err(e) = run_pipeline(cfg, cons, input_rate, channels, proc_stop, proc_sink.clone()) {
                        tracing::error!("audio pipeline stopped: {e}");
                        proc_sink.error(track, e.to_string());
                    }
                })
                .map_err(|e| AppError::Audio(format!("could not spawn pipeline thread: {e}")))?,
        );
    }

    Ok(CaptureHandle { stop, threads })
}

/// Owns the `cpal::Stream`. Streams are not `Send` on macOS, so one is created, held and
/// dropped on the same thread. This thread also rebuilds the stream when a device goes away
/// — headphones unplugged, output device switched.
fn run_device(
    track: Track,
    device_id: String,
    supported: cpal::SupportedStreamConfig,
    prod: Arc<Mutex<HeapProd<f32>>>,
    stop: Arc<AtomicBool>,
    sink: Arc<dyn CaptureSink>,
) {
    let mut attempt = 0u32;
    while !stop.load(Ordering::SeqCst) {
        let failed = Arc::new(AtomicBool::new(false));
        match build_stream(&device_id, &supported, prod.clone(), failed.clone()) {
            Ok(stream) => {
                if let Err(e) = stream.play() {
                    sink.error(track, format!("could not start stream: {e}"));
                    failed.store(true, Ordering::SeqCst);
                } else {
                    attempt = 0;
                    tracing::info!("stream '{device_id}' ({}) running", track.as_str());
                }
                while !stop.load(Ordering::SeqCst) && !failed.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                drop(stream);
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                sink.error(track, "audio stream dropped, reconnecting".into());
            }
            Err(e) => {
                attempt += 1;
                if attempt > 3 {
                    sink.error(track, format!("could not open device '{device_id}': {e}"));
                    return;
                }
                std::thread::sleep(Duration::from_millis(300 * attempt as u64));
            }
        }
    }
}

fn build_stream(
    device_id: &str,
    supported: &cpal::SupportedStreamConfig,
    prod: Arc<Mutex<HeapProd<f32>>>,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream> {
    let device = find_device(device_id)?;
    let config: cpal::StreamConfig = supported.config();
    let channels = config.channels as usize;
    let err_flag = failed.clone();
    let err_fn = move |e| {
        tracing::error!("cpal stream error: {e}");
        err_flag.store(true, Ordering::SeqCst);
    };

    macro_rules! build {
        ($t:ty) => {{
            device
                .build_input_stream(
                    config.clone(),
                    move |data: &[$t], _: &cpal::InputCallbackInfo| {
                        push_mono(data, channels, &prod);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| AppError::Audio(format!("build stream: {e}")))
        }};
    }

    match supported.sample_format() {
        cpal::SampleFormat::F32 => build!(f32),
        cpal::SampleFormat::I16 => build!(i16),
        cpal::SampleFormat::U16 => build!(u16),
        cpal::SampleFormat::I32 => build!(i32),
        other => Err(AppError::Audio(format!(
            "sample format {other:?} is not supported"
        ))),
    }
}

/// Called from the audio callback. Must not allocate and must not block: the scratch buffer
/// lives on the stack and the lock is taken with `try_lock`.
fn push_mono<T: cpal::Sample + cpal::SizedSample>(
    data: &[T],
    channels: usize,
    prod: &Mutex<HeapProd<f32>>,
) where
    f32: cpal::FromSample<T>,
{
    const CHUNK: usize = 512;
    let mut scratch = [0.0f32; CHUNK];
    let Ok(mut p) = prod.try_lock() else {
        return; // only happens while the stream is being rebuilt
    };
    let mut n = 0usize;
    for frame in data.chunks(channels) {
        let mut acc = 0.0f32;
        for s in frame {
            acc += <f32 as cpal::FromSample<T>>::from_sample_(*s);
        }
        scratch[n] = acc / frame.len() as f32;
        n += 1;
        if n == CHUNK {
            p.push_slice(&scratch[..n]);
            n = 0;
        }
    }
    if n > 0 {
        p.push_slice(&scratch[..n]);
    }
}

fn run_pipeline(
    cfg: TrackConfig,
    mut cons: HeapCons<f32>,
    input_rate: u32,
    _channels: usize,
    stop: Arc<AtomicBool>,
    sink: Arc<dyn CaptureSink>,
) -> Result<()> {
    let mut resampler = ToTarget::new(input_rate, TARGET_RATE)?;
    let mut writer = WavTrackWriter::create(&cfg.wav_path)?;
    let mut segmenter = Segmenter::new(cfg.track);

    let mut raw = vec![0.0f32; input_rate as usize / 5]; // 200 ms
    let mut resampled: Vec<f32> = Vec::with_capacity(TARGET_RATE as usize);
    let mut utterances: Vec<Utterance> = Vec::new();

    let mut last_level = Instant::now();
    let mut last_partial = Instant::now();
    let mut level_acc = 0.0f64;
    let mut level_n = 0usize;
    let mut silent_since: Option<Instant> = None;
    let mut silence_warned = false;

    while !stop.load(Ordering::SeqCst) {
        let n = cons.pop_slice(&mut raw);
        if n == 0 {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }

        resampled.clear();
        resampler.push(&raw[..n], &mut resampled)?;
        if resampled.is_empty() {
            continue;
        }

        writer.write(&resampled)?;

        for &s in resampled.iter() {
            level_acc += (s * s) as f64;
        }
        level_n += resampled.len();

        if cfg.segment {
            utterances.clear();
            segmenter.push(&resampled, &mut utterances);
            for u in utterances.drain(..) {
                sink.utterance(u);
            }
        }

        if last_level.elapsed() >= LEVEL_INTERVAL && level_n > 0 {
            let rms = (level_acc / level_n as f64).sqrt() as f32;
            let db = 20.0 * (rms + 1e-10).log10();
            sink.level(cfg.track, db);
            level_acc = 0.0;
            level_n = 0;
            last_level = Instant::now();

            // The "no audio" warning only makes sense for the system track: if someone
            // forgets to route output to the capture device, everything is silent.
            if cfg.track == Track::System {
                if db < SILENCE_DB {
                    let since = *silent_since.get_or_insert_with(Instant::now);
                    let secs = since.elapsed().as_secs();
                    if secs >= SILENCE_WARN_SECS && !silence_warned {
                        silence_warned = true;
                        sink.silent(cfg.track, secs);
                    }
                } else {
                    silent_since = None;
                    silence_warned = false;
                }
            }
        }

        if cfg.emit_partials && last_partial.elapsed() >= PARTIAL_INTERVAL {
            last_partial = Instant::now();
            if let Some((start_ms, pcm)) = segmenter.partial_snapshot(PARTIAL_MIN_SAMPLES) {
                sink.partial(cfg.track, start_ms, pcm);
            }
        }
    }

    // Drain the buffer so the last sentence is not lost when stop is pressed.
    loop {
        let n = cons.pop_slice(&mut raw);
        if n == 0 {
            break;
        }
        resampled.clear();
        resampler.push(&raw[..n], &mut resampled)?;
        writer.write(&resampled)?;
        if cfg.segment {
            utterances.clear();
            segmenter.push(&resampled, &mut utterances);
            for u in utterances.drain(..) {
                sink.utterance(u);
            }
        }
    }
    if cfg.segment {
        utterances.clear();
        segmenter.flush(&mut utterances);
        for u in utterances.drain(..) {
            sink.utterance(u);
        }
    }
    writer.finalize()?;
    tracing::info!(
        "pipeline {} finished (noise floor {:.1} dBFS)",
        cfg.track.as_str(),
        segmenter.noise_floor_db()
    );
    Ok(())
}
