use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("audio: {0}")]
    Audio(String),
    #[error("model: {0}")]
    Model(String),
    #[error("database: {0}")]
    Db(String),
    #[error("io: {0}")]
    Io(String),
    #[error("configuration: {0}")]
    Config(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("busy: {0}")]
    Busy(String),
    #[error("feature disabled: {0}")]
    Disabled(String),
    #[error("network: {0}")]
    Http(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Audio(_) => "audio",
            AppError::Model(_) => "model",
            AppError::Db(_) => "db",
            AppError::Io(_) => "io",
            AppError::Config(_) => "config",
            AppError::NotFound(_) => "not_found",
            AppError::Busy(_) => "busy",
            AppError::Disabled(_) => "disabled",
            AppError::Http(_) => "http",
        }
    }
}

/// Sent to the frontend as `{ code, message }` so the UI can distinguish error kinds without
/// matching on strings.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}
impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Db(e.to_string())
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Config(e.to_string())
    }
}
impl From<hound::Error> for AppError {
    fn from(e: hound::Error) -> Self {
        AppError::Io(format!("wav: {e}"))
    }
}
impl From<whisper_rs::WhisperError> for AppError {
    fn from(e: whisper_rs::WhisperError) -> Self {
        AppError::Model(e.to_string())
    }
}
impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Http(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
