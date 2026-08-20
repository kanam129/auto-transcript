use crate::error::{AppError, Result};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[derive(Debug, Clone)]
pub struct RawSegment {
    pub text: String,
    pub t0_ms: i64,
    pub t1_ms: i64,
    pub no_speech: f32,
}

#[derive(Debug, Clone)]
pub struct TranscribeOutput {
    pub segments: Vec<RawSegment>,
    /// The language Whisper actually used for this chunk.
    pub lang: String,
}

#[derive(Debug, Clone)]
pub struct TranscribeOpts<'a> {
    /// `None` lets Whisper detect it.
    pub lang: Option<&'a str>,
    pub initial_prompt: Option<&'a str>,
    pub n_threads: i32,
    /// For previews: a single segment, which is faster.
    pub single_segment: bool,
    /// Trims the encoder window so short chunks do not pay the full 30-second cost.
    /// `0` means the full window, which is Whisper's default.
    pub audio_ctx: i32,
}

/// The encoder window size that fits `n` samples of 16 kHz audio.
///
/// Whisper always runs its encoder over a full 30-second window regardless of how long the
/// audio actually is — that is the fixed ~1.8 s cost that makes short chunks feel slow.
/// `audio_ctx` trims that window: 1500 units is 30 seconds, so 50 units per second.
///
/// The 300-unit (6-second) margin is not arbitrary. Measurements in `examples/latency.rs`
/// show an 8-second chunk still intact at 700 but repeating words at 528, and the floor of
/// 768 was set after `examples/sim.rs` caught the model emitting one sentence three times
/// in a row at 512.
pub fn audio_ctx_for_samples(n: usize) -> i32 {
    let secs = n as f32 / crate::audio::TARGET_RATE as f32;
    let want = (secs * 50.0).ceil() as i32 + 300;
    // Rounded to a handful of fixed sizes rather than an exact value. Every new window size
    // forces whisper.cpp to rebuild its Metal compute buffers, and that costs far more than
    // a tightly-fitted window saves. With few distinct sizes, the buffers get reused.
    for bucket in [768, 1024, 1280] {
        if want <= bucket {
            return bucket;
        }
    }
    1500
}

pub struct WhisperEngine {
    ctx: WhisperContext,
    model_id: String,
}

impl WhisperEngine {
    pub fn load(model_id: &str) -> Result<Self> {
        let path = crate::stt::model_manager::model_path(model_id)?;
        if !path.exists() {
            return Err(AppError::Model(format!("model '{model_id}' has not been downloaded")));
        }
        let mut cparams = WhisperContextParameters::default();
        cparams.use_gpu(true);
        // Flash attention on Metal. Can be turned off with AUTO_TRANSCRIPT_FLASH_ATTN=0 if
        // it ever turns out to misbehave on some device.
        let flash = std::env::var("AUTO_TRANSCRIPT_FLASH_ATTN")
            .map(|v| v != "0")
            .unwrap_or(true);
        cparams.flash_attn(flash);
        let ctx = WhisperContext::new_with_params(
            path.to_str()
                .ok_or_else(|| AppError::Model("invalid model path".into()))?,
            cparams,
        )?;
        tracing::info!("loaded model '{model_id}' from {}", path.display());
        Ok(Self {
            ctx,
            model_id: model_id.to_string(),
        })
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn transcribe(&self, pcm: &[f32], opts: &TranscribeOpts) -> Result<TranscribeOutput> {
        let mut state = self.ctx.create_state()?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        params.set_n_threads(opts.n_threads);
        params.set_translate(false);
        // No context carried between chunks: this stops a hallucination in one chunk from
        // seeding the next, which is the main source of junk text from Whisper.
        params.set_no_context(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_temperature(0.0);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_single_segment(opts.single_segment);
        if opts.audio_ctx > 0 {
            params.set_audio_ctx(opts.audio_ctx);
        }
        params.set_language(Some(opts.lang.unwrap_or("auto")));
        if let Some(p) = opts.initial_prompt {
            if !p.trim().is_empty() {
                params.set_initial_prompt(p);
            }
        }

        state.full(params, pcm)?;

        let n = state.full_n_segments();
        let mut segments = Vec::with_capacity(n.max(0) as usize);
        for i in 0..n {
            let Some(seg) = state.get_segment(i) else {
                continue;
            };
            let text = seg.to_str_lossy()?.to_string();
            segments.push(RawSegment {
                text,
                t0_ms: seg.start_timestamp() * 10, // centidetik → milidetik
                t1_ms: seg.end_timestamp() * 10,
                no_speech: seg.no_speech_probability(),
            });
        }

        let lang_id = state.full_lang_id_from_state();
        let lang = whisper_rs::get_lang_str(lang_id)
            .unwrap_or("unknown")
            .to_string();

        Ok(TranscribeOutput { segments, lang })
    }
}
