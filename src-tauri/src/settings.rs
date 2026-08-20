use crate::error::Result;
use crate::paths;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LangMode {
    Auto,
    En,
    Id,
}

impl LangMode {
    /// `None` lets Whisper detect the language itself.
    pub fn forced(&self) -> Option<&'static str> {
        match self {
            LangMode::Auto => None,
            LangMode::En => Some("en"),
            LangMode::Id => Some("id"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSettings {
    pub base_url: String,
    pub model: String,
    pub timeout_secs: u64,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            timeout_secs: 120,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub system_source_id: Option<String>,
    pub mic_source_id: Option<String>,
    /// The microphone is still transcribed (for summary quality) but never shown on screen.
    pub mic_transcribe: bool,
    pub model_id: String,
    /// Model used for preview text. Kept separate from the main model so previews stay
    /// quick; BENCHMARK.md explains the choice.
    pub partial_model_id: String,
    pub partials_enabled: bool,
    pub lang_mode: LangMode,
    pub font_size: u32,
    pub always_on_top: bool,
    pub keep_audio_days: Option<u32>,
    pub keep_audio: bool,
    /// Domain terms and participant names, passed to Whisper as an `initial_prompt`.
    pub vocabulary: String,
    pub summary_enabled: bool,
    pub llm: LlmSettings,
    /// Whether the first-run screen has been completed.
    pub onboarded: bool,
    /// Settings schema version, used for migrations between releases.
    ///
    /// Explicitly defaults to 0, not `SETTINGS_VERSION`. Struct-level `#[serde(default)]`
    /// fills missing fields from `Settings::default()`, so without this an older settings
    /// file — one written before this field existed — would claim to be current and its
    /// migration would be skipped silently.
    #[serde(default = "version_of_legacy_file")]
    pub version: u32,
}

/// Current settings schema version. Bump it together with a branch in `migrate()`.
const SETTINGS_VERSION: u32 = 1;

fn version_of_legacy_file() -> u32 {
    0
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            system_source_id: None,
            mic_source_id: None,
            mic_transcribe: true,
            model_id: "large-v3-turbo-q5_0".into(),
            partial_model_id: "small-q5_1".into(),
            partials_enabled: true,
            lang_mode: LangMode::Auto,
            font_size: 18,
            always_on_top: false,
            keep_audio_days: None,
            keep_audio: true,
            vocabulary: String::new(),
            summary_enabled: false,
            llm: LlmSettings::default(),
            onboarded: false,
            version: SETTINGS_VERSION,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        match Self::try_load() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("could not load settings, falling back to defaults: {e}");
                Self::default()
            }
        }
    }

    fn try_load() -> Result<Self> {
        let path = paths::settings_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path)?;
        let mut settings: Self = serde_json::from_str(&raw)?;
        if settings.migrate() {
            settings.save()?;
        }
        Ok(settings)
    }

    /// Brings older settings up to the current version. Returns `true` if anything changed
    /// and therefore needs writing back to disk.
    fn migrate(&mut self) -> bool {
        let mut changed = false;
        if self.version < 1 {
            // The preview model moves up from `base` to `small`. Measurements in
            // BENCHMARK.md show `base` inventing wrong text on mixed-language audio — it
            // once emitted Russian — while `small` costs only about 80 ms more.
            if self.partial_model_id == "base-q5_1" {
                self.partial_model_id = "small-q5_1".into();
                tracing::info!("settings migration: preview model base-q5_1 -> small-q5_1");
            }
            self.version = 1;
            changed = true;
        }
        changed
    }

    pub fn save(&self) -> Result<()> {
        let path = paths::settings_path()?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }
}
