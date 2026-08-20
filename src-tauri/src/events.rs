use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const TRANSCRIPT_PARTIAL: &str = "transcript:partial";
pub const TRANSCRIPT_SEGMENT: &str = "transcript:segment";
pub const AUDIO_LEVEL: &str = "audio:level";
pub const AUDIO_SILENT: &str = "audio:silent";
pub const AUDIO_ERROR: &str = "audio:error";
pub const MODEL_DOWNLOAD: &str = "model:download";
pub const ENGINE_DEGRADED: &str = "engine:degraded";
pub const SESSION_STATE: &str = "session:state";
pub const SUMMARY_PROGRESS: &str = "summary:progress";
pub const SUMMARY_READY: &str = "summary:ready";
pub const MIC_PROGRESS: &str = "mic:progress";

pub fn emit<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::warn!("could not emit event {event}: {e}");
    }
}

#[derive(Serialize, Clone)]
pub struct SegmentEvent {
    pub session_id: String,
    pub id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub lang: Option<String>,
    pub confidence: Option<f32>,
}

#[derive(Serialize, Clone)]
pub struct PartialEvent {
    pub session_id: String,
    pub start_ms: i64,
    pub text: String,
}

#[derive(Serialize, Clone)]
pub struct LevelEvent {
    pub track: &'static str,
    pub db: f32,
}

#[derive(Serialize, Clone)]
pub struct SilentEvent {
    pub track: &'static str,
    pub seconds: u64,
}

#[derive(Serialize, Clone)]
pub struct ErrorEvent {
    pub code: &'static str,
    pub message: String,
}

#[derive(Serialize, Clone)]
pub struct DownloadEvent {
    pub model_id: String,
    pub downloaded: u64,
    pub total: u64,
}

#[derive(Serialize, Clone)]
pub struct DegradedEvent {
    pub from: String,
    pub to: String,
    pub reason: String,
}

#[derive(Serialize, Clone)]
pub struct SummaryProgressEvent {
    pub session_id: String,
    pub stage: String,
    pub done: usize,
    pub total: usize,
}
