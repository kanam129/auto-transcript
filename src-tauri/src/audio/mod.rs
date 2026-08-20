pub mod capture;
pub mod devices;
pub mod resample;
pub mod segmenter;
pub mod vad;
pub mod writer;

use serde::{Deserialize, Serialize};

/// The two audio paths that run side by side. `System` is shown on screen; `Mic` is only
/// transcribed as material for summaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Track {
    System,
    Mic,
}

impl Track {
    pub fn as_str(&self) -> &'static str {
        match self {
            Track::System => "system",
            Track::Mic => "mic",
        }
    }
}

/// Where a source takes its audio from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// An ordinary input device: a microphone.
    Mic,
    /// An *output* device tapped directly. cpal implements this as a CoreAudio process
    /// tap on macOS and WASAPI loopback on Windows — no extra driver, no change to the
    /// user's output device, and volume keys keep working.
    SystemOutput,
    /// A virtual driver such as BlackHole or a Multi-Output Device. Still supported as a
    /// fallback when direct capture is unavailable or its permission is refused.
    VirtualLoopback,
}

impl SourceKind {
    /// Whether this source captures audio coming out of the computer.
    pub fn captures_system_audio(&self) -> bool {
        matches!(self, SourceKind::SystemOutput | SourceKind::VirtualLoopback)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    pub kind: SourceKind,
    pub sample_rate: u32,
    pub channels: u16,
    pub is_default: bool,
}

/// A chunk of speech ready for Whisper. The PCM is already 16 kHz mono f32.
#[derive(Debug)]
pub struct Utterance {
    pub track: Track,
    pub start_ms: i64,
    pub end_ms: i64,
    pub pcm: Vec<f32>,
}

pub const TARGET_RATE: u32 = 16_000;
pub const FRAME_SAMPLES: usize = 320; // 20 ms @ 16 kHz

/// Device names that indicate a loopback path (capturing system output).
const LOOPBACK_HINTS: [&str; 6] = [
    "blackhole",
    "loopback",
    "soundflower",
    "aggregate",
    "multi-output",
    "existential",
];

pub fn looks_like_loopback(name: &str) -> bool {
    let lower = name.to_lowercase();
    LOOPBACK_HINTS.iter().any(|h| lower.contains(h))
}
