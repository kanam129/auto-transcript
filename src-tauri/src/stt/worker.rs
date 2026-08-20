use super::engine::{audio_ctx_for_samples, TranscribeOpts, WhisperEngine};
use super::filter;
use crate::audio::Track;
use crate::error::Result;
use crate::settings::LangMode;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct FinalJob {
    pub track: Track,
    pub start_ms: i64,
    pub end_ms: i64,
    pub pcm: Vec<f32>,
}

#[derive(Debug)]
pub struct PartialJob {
    pub start_ms: i64,
    pub pcm: Vec<f32>,
}

pub trait TranscriptSink: Send + Sync + 'static {
    fn final_segment(
        &self,
        track: Track,
        start_ms: i64,
        end_ms: i64,
        text: String,
        lang: String,
        confidence: f32,
    );
    fn partial(&self, start_ms: i64, text: String);
    fn degraded(&self, from: String, to: String, reason: String);
    fn error(&self, msg: String);
}

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub model_id: String,
    pub fallback_model_id: String,
    pub partial_model_id: String,
    pub partials_enabled: bool,
    pub lang_mode: LangMode,
    pub vocabulary: String,
    pub n_threads: i32,
}

/// Queue depth that, if sustained for `DEGRADE_AFTER`, triggers a model downgrade. The
/// number is small because one job is one utterance: four of them stacked up already means
/// we are well over a minute behind.
const DEGRADE_DEPTH: usize = 3;
const DEGRADE_AFTER: Duration = Duration::from_secs(30);
const RECOVER_AFTER: Duration = Duration::from_secs(120);
/// The preview window is kept short so it stays cheap: 6 seconds through the small model
/// finishes in about 0.2 s, comfortably fast enough to refresh every 0.7 s.
const PARTIAL_MAX_SAMPLES: usize = crate::audio::TARGET_RATE as usize * 6;

/// The most recently detected language, shared between the preview worker (which detects
/// it cheaply) and the final worker (which uses it as a forced language).
///
/// This replaces automatic detection inside the large model, which costs a full extra
/// encoder pass — roughly 1.75 s per chunk, about half of the total latency.
#[derive(Clone)]
pub struct SharedLang(Arc<std::sync::Mutex<Option<String>>>);

impl SharedLang {
    fn new() -> Self {
        Self(Arc::new(std::sync::Mutex::new(None)))
    }
    fn get(&self) -> Option<String> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
    fn set(&self, lang: &str) {
        if let Ok(mut g) = self.0.lock() {
            if g.as_deref() != Some(lang) {
                tracing::debug!("active language is now '{lang}'");
            }
            *g = Some(lang.to_string());
        }
    }
}

pub struct Transcribers {
    hi_tx: Sender<FinalJob>,
    lo_tx: Sender<FinalJob>,
    partial_tx: std::sync::Mutex<Option<Sender<PartialJob>>>,
    pending: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    threads: std::sync::Mutex<Vec<JoinHandle<()>>>,
}

impl Transcribers {
    pub fn start(cfg: WorkerConfig, sink: Arc<dyn TranscriptSink>) -> Result<Self> {
        let (hi_tx, hi_rx) = unbounded::<FinalJob>();
        let (lo_tx, lo_rx) = unbounded::<FinalJob>();
        let pending = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let mut threads = Vec::new();

        // Load the main model on the calling thread so failures (model not downloaded,
        // corrupt file) surface to the user instead of disappearing inside a thread.
        let engine = Arc::new(WhisperEngine::load(&cfg.model_id)?);
        let shared_lang = SharedLang::new();
        let degraded = Arc::new(AtomicBool::new(false));

        {
            let cfg = cfg.clone();
            let sink = sink.clone();
            let pending = pending.clone();
            let stop = stop.clone();
            let lang = shared_lang.clone();
            let deg = degraded.clone();
            let engine = engine.clone();
            threads.push(
                std::thread::Builder::new()
                    .name("stt-final".into())
                    .spawn(move || {
                        run_final_worker(
                            engine,
                            cfg,
                            FinalChannels {
                                hi_rx,
                                lo_rx,
                                pending,
                                stop,
                            },
                            sink,
                            lang,
                            deg,
                        )
                    })
                    .map_err(|e| {
                        crate::error::AppError::Model(format!("could not spawn transcription thread: {e}"))
                    })?,
            );
        }

        let partial_tx = if cfg.partials_enabled {
            let (tx, rx) = bounded::<PartialJob>(1);
            // If the preview model is the same as the main one, share the context:
            // whisper.cpp is safe across threads as long as each call has its own state,
            // and this avoids loading a large model twice.
            let partial_engine = if cfg.partial_model_id == cfg.model_id {
                Ok(engine.clone())
            } else {
                WhisperEngine::load(&cfg.partial_model_id).map(Arc::new)
            };
            match partial_engine {
                Ok(pe) => {
                    let cfg2 = cfg.clone();
                    let sink2 = sink.clone();
                    let pending2 = pending.clone();
                    let stop2 = stop.clone();
                    let lang2 = shared_lang.clone();
                    let deg2 = degraded.clone();
                    threads.push(
                        std::thread::Builder::new()
                            .name("stt-partial".into())
                            .spawn(move || {
                                run_partial_worker(
                                    pe,
                                    cfg2,
                                    PartialChannels {
                                        rx,
                                        pending: pending2,
                                        stop: stop2,
                                        degraded: deg2,
                                    },
                                    sink2,
                                    lang2,
                                )
                            })
                            .map_err(|e| {
                                crate::error::AppError::Model(format!(
                                    "could not spawn preview thread: {e}"
                                ))
                            })?,
                    );
                    Some(tx)
                }
                Err(e) => {
                    // Previews are a luxury, not a requirement. If the model is missing,
                    // the session still runs with final text only.
                    tracing::warn!("preview disabled: {e}");
                    sink.error(format!("Preview disabled: {e}"));
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            hi_tx,
            lo_tx,
            partial_tx: std::sync::Mutex::new(partial_tx),
            pending,
            stop,
            threads: std::sync::Mutex::new(threads),
        })
    }

    pub fn submit_final(&self, job: FinalJob) {
        self.pending.fetch_add(1, Ordering::SeqCst);
        let tx = match job.track {
            Track::System => &self.hi_tx,
            Track::Mic => &self.lo_tx,
        };
        if tx.send(job).is_err() {
            self.pending.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Previews are droppable: if the worker is busy the job is discarded, because a
    /// fresher and more relevant snapshot will arrive a moment later.
    pub fn submit_partial(&self, job: PartialJob) {
        if let Ok(guard) = self.partial_tx.lock() {
            if let Some(tx) = guard.as_ref() {
                let _ = tx.try_send(job);
            }
        }
    }

    pub fn queue_depth(&self) -> usize {
        self.pending.load(Ordering::SeqCst)
    }

    /// Waits for the final queue to drain (up to `timeout`), then stops the workers.
    /// This is what keeps the last sentence of a meeting in the transcript.
    pub fn shutdown(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while self.pending.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        let left = self.pending.load(Ordering::SeqCst);
        if left > 0 {
            tracing::warn!("{left} audio chunks were still queued when recording stopped");
        }
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(mut g) = self.partial_tx.lock() {
            drop(g.take());
        }
        if let Ok(mut g) = self.threads.lock() {
            for t in g.drain(..) {
                let _ = t.join();
            }
        }
    }
}

/// Picks the language and encoder window size for one chunk.
///
/// The rule: if the language is already known — forced by the user, or just detected by the
/// preview worker — the encoder window may be trimmed, which takes an 8-second chunk from
/// 1.9 s to 0.8 s. If it is not known, we pay full price, because combining automatic
/// detection with a trimmed window measurably corrupts mixed-language speech: English
/// sentences come back as Indonesian.
fn plan_for(cfg: &WorkerConfig, known: Option<String>, n_samples: usize) -> (Option<String>, i32) {
    match cfg.lang_mode.forced() {
        Some(l) => (Some(l.to_string()), audio_ctx_for_samples(n_samples)),
        None => match known {
            Some(l) => (Some(l), audio_ctx_for_samples(n_samples)),
            None => (None, 0),
        },
    }
}

/// Runs a few empty inferences so Metal allocates its compute buffers now rather than when
/// the first sentence of a meeting arrives.
///
/// Without this, `examples/sim.rs` shows the first segment consistently taking about 4 s
/// while later ones take about 1.8 s. Warming up moves that cost to a moment when nobody
/// is waiting.
fn warm_up(engine: &WhisperEngine, n_threads: i32) {
    let started = Instant::now();
    for secs in [2.0f32, 5.0] {
        let pcm = vec![0.0f32; (crate::audio::TARGET_RATE as f32 * secs) as usize];
        let _ = engine.transcribe(
            &pcm,
            &TranscribeOpts {
                lang: Some("en"),
                initial_prompt: None,
                n_threads,
                single_segment: true,
                audio_ctx: audio_ctx_for_samples(pcm.len()),
            },
        );
    }
    tracing::info!(
        "warmed up model '{}' in {} ms",
        engine.model_id(),
        started.elapsed().as_millis()
    );
}

/// Inputs for the final worker: two priority queues plus the depth counter it shares with
/// the preview worker.
struct FinalChannels {
    hi_rx: Receiver<FinalJob>,
    lo_rx: Receiver<FinalJob>,
    pending: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
}

fn run_final_worker(
    mut engine: Arc<WhisperEngine>,
    cfg: WorkerConfig,
    ch: FinalChannels,
    sink: Arc<dyn TranscriptSink>,
    shared_lang: SharedLang,
    degraded_flag: Arc<AtomicBool>,
) {
    let FinalChannels {
        hi_rx,
        lo_rx,
        pending,
        stop,
    } = ch;
    warm_up(&engine, cfg.n_threads);

    let mut dedup = DedupGuard::default();
    let mut deep_since: Option<Instant> = None;
    let mut shallow_since: Option<Instant> = None;
    let mut degraded = false;
    let original_model = cfg.model_id.clone();

    loop {
        // The system track always wins: that is what someone is reading on screen.
        // The microphone track may lag, since its output is only used for summaries.
        let job = hi_rx.try_recv().ok().or_else(|| lo_rx.try_recv().ok());

        let Some(job) = job else {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            continue;
        };

        let (lang, audio_ctx) = plan_for(&cfg, shared_lang.get(), job.pcm.len());
        let opts = TranscribeOpts {
            lang: lang.as_deref(),
            initial_prompt: Some(&cfg.vocabulary),
            n_threads: cfg.n_threads,
            single_segment: false,
            audio_ctx,
        };

        let started = Instant::now();
        match engine.transcribe(&job.pcm, &opts) {
            Ok(out) => {
                // When we had to fall back to automatic detection, restrict the result to
                // the two languages this app actually works in.
                if lang.is_none() && (out.lang == "en" || out.lang == "id") {
                    shared_lang.set(&out.lang);
                }
                tracing::debug!(
                    "transcribed a {} ms chunk in {} ms (lang {}, ctx {})",
                    job.end_ms - job.start_ms,
                    started.elapsed().as_millis(),
                    out.lang,
                    audio_ctx
                );
                emit_segments(&sink, &job, out, &mut dedup);
            }
            Err(e) => {
                tracing::error!("transcription failed: {e}");
                sink.error(format!("Transcription failed: {e}"));
            }
        }
        pending.fetch_sub(1, Ordering::SeqCst);

        // Load watch: if the queue stays deep, drop to a smaller model rather than fall
        // further behind; go back up once things have been calm for a while.
        let depth = pending.load(Ordering::SeqCst);
        if depth > DEGRADE_DEPTH {
            shallow_since = None;
            let since = *deep_since.get_or_insert_with(Instant::now);
            if !degraded
                && since.elapsed() >= DEGRADE_AFTER
                && cfg.fallback_model_id != engine.model_id()
            {
                match WhisperEngine::load(&cfg.fallback_model_id).map(Arc::new) {
                    Ok(e) => {
                        let from = engine.model_id().to_string();
                        engine = e;
                        degraded = true;
                        degraded_flag.store(true, Ordering::SeqCst);
                        deep_since = None;
                        sink.degraded(
                            from,
                            cfg.fallback_model_id.clone(),
                            format!("{depth} chunks behind"),
                        );
                    }
                    Err(e) => tracing::warn!("could not switch to the fallback model: {e}"),
                }
            }
        } else {
            deep_since = None;
            if degraded && depth == 0 {
                let since = *shallow_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= RECOVER_AFTER {
                    match WhisperEngine::load(&original_model).map(Arc::new) {
                        Ok(e) => {
                            let from = engine.model_id().to_string();
                            engine = e;
                            degraded = false;
                            degraded_flag.store(false, Ordering::SeqCst);
                            shallow_since = None;
                            sink.degraded(from, original_model.clone(), "load has settled".into());
                        }
                        Err(e) => tracing::warn!("could not switch back to the main model: {e}"),
                    }
                }
            } else if depth > 0 {
                shallow_since = None;
            }
        }
    }
    tracing::info!("final transcription worker stopped");
}

/// Suppresses a sentence that is identical to the one before it.
///
/// Whisper occasionally gets stuck in a loop and emits the same sentence repeatedly — caught
/// directly by `examples/sim.rs`, where one chunk produced an identical sentence three times.
/// People essentially never repeat a whole sentence verbatim twice in a row, so dropping it
/// is safe.
#[derive(Default)]
struct DedupGuard {
    last: Option<String>,
}

impl DedupGuard {
    fn accept(&mut self, text: &str) -> bool {
        let key = filter::normalize(text);
        if self.last.as_deref() == Some(key.as_str()) {
            tracing::debug!("dropped duplicate segment: {text}");
            return false;
        }
        self.last = Some(key);
        true
    }
}

fn emit_segments(
    sink: &Arc<dyn TranscriptSink>,
    job: &FinalJob,
    out: super::engine::TranscribeOutput,
    dedup: &mut DedupGuard,
) {
    for seg in out.segments {
        let Some(text) = filter::accept(&seg.text, seg.no_speech) else {
            continue;
        };
        if !dedup.accept(&text) {
            continue;
        }
        // Whisper timestamps are relative to the chunk; shift them into session time.
        let start = job.start_ms + seg.t0_ms;
        let end = (job.start_ms + seg.t1_ms).min(job.end_ms).max(start);
        sink.final_segment(
            job.track,
            start,
            end,
            text,
            out.lang.clone(),
            1.0 - seg.no_speech,
        );
    }
}

/// Stops the beginning of the preview text from rewriting itself.
///
/// On every tick the small model re-transcribes the whole chunk from scratch, so the opening
/// words get rewritten and flicker: "Also case of the main blocker" becomes "All case so the
/// main blocker". That flicker is what makes the app feel slow — not the latency figure.
///
/// How it works (LocalAgreement-2): words at the start that two consecutive hypotheses agree
/// on are treated as settled and locked. The head of the sentence goes still while the tail
/// stays live, which is exactly how good broadcast captioning behaves.
#[derive(Default)]
struct PrefixLock {
    utterance_start: i64,
    previous: Vec<String>,
    locked: Vec<String>,
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(|w| w.to_string()).collect()
}

fn common_prefix_len(a: &[String], b: &[String]) -> usize {
    a.iter()
        .zip(b.iter())
        .take_while(|(x, y)| {
            x.trim_matches(|c: char| !c.is_alphanumeric())
                .eq_ignore_ascii_case(y.trim_matches(|c: char| !c.is_alphanumeric()))
        })
        .count()
}

impl PrefixLock {
    fn apply(&mut self, start_ms: i64, hypothesis: &str) -> String {
        if start_ms != self.utterance_start {
            self.utterance_start = start_ms;
            self.previous.clear();
            self.locked.clear();
        }
        let hyp = words(hypothesis);
        let agreed = common_prefix_len(&hyp, &self.previous);
        if agreed > self.locked.len() {
            self.locked = hyp[..agreed].to_vec();
        }
        self.previous = hyp.clone();

        // The new hypothesis is shorter than what is already locked — show it as-is rather
        // than splicing something awkward together.
        if hyp.len() < self.locked.len() {
            return hypothesis.to_string();
        }
        let mut out = self.locked.clone();
        out.extend_from_slice(&hyp[self.locked.len()..]);
        out.join(" ")
    }
}

/// Inputs for the preview worker, including the two flags that let it know when to get out
/// of the way.
struct PartialChannels {
    rx: Receiver<PartialJob>,
    pending: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    degraded: Arc<AtomicBool>,
}

fn run_partial_worker(
    engine: Arc<WhisperEngine>,
    cfg: WorkerConfig,
    ch: PartialChannels,
    sink: Arc<dyn TranscriptSink>,
    shared_lang: SharedLang,
) {
    let PartialChannels {
        rx,
        pending,
        stop,
        degraded: degraded_flag,
    } = ch;
    warm_up(&engine, cfg.n_threads);
    let mut lock = PrefixLock::default();

    while !stop.load(Ordering::SeqCst) {
        let Ok(job) = rx.recv_timeout(Duration::from_millis(200)) else {
            continue;
        };
        // Do not take the GPU when final text is piling up. The threshold is 1, not 0: one
        // final job in flight is normal, and a preview that stops whenever a single job is
        // queued would essentially never appear.
        if pending.load(Ordering::SeqCst) > 1 {
            continue;
        }
        // When the engine is struggling, previews are the first thing to sacrifice.
        if degraded_flag.load(Ordering::SeqCst) {
            continue;
        }
        let pcm = if job.pcm.len() > PARTIAL_MAX_SAMPLES {
            &job.pcm[job.pcm.len() - PARTIAL_MAX_SAMPLES..]
        } else {
            &job.pcm[..]
        };
        // This small model doubles as the language detector for the final worker: on a
        // 4-second window it finishes in about 0.2 s, against roughly 1.75 s if the large
        // model detects for itself.
        let opts = TranscribeOpts {
            lang: cfg.lang_mode.forced(),
            initial_prompt: Some(&cfg.vocabulary),
            n_threads: cfg.n_threads,
            single_segment: false,
            audio_ctx: audio_ctx_for_samples(pcm.len()),
        };
        match engine.transcribe(pcm, &opts) {
            Ok(out) => {
                if cfg.lang_mode.forced().is_none() && (out.lang == "en" || out.lang == "id") {
                    shared_lang.set(&out.lang);
                }
                let text: String = out
                    .segments
                    .iter()
                    .map(|s| s.text.trim())
                    .collect::<Vec<_>>()
                    .join(" ");
                if let Some(clean) = filter::accept(&text, 0.0) {
                    let stable = lock.apply(job.start_ms, &clean);
                    sink.partial(job.start_ms, stable);
                }
            }
            Err(e) => tracing::debug!("preview failed: {e}"),
        }
    }
    tracing::info!("preview worker stopped");
}
