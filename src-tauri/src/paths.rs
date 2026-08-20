use crate::error::{AppError, Result};
use std::path::PathBuf;

fn base() -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .ok_or_else(|| AppError::Io("could not locate the application data directory".into()))?
        .join("auto-transcript");
    Ok(dir)
}

fn ensure(p: PathBuf) -> Result<PathBuf> {
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

pub fn data_dir() -> Result<PathBuf> {
    ensure(base()?)
}

pub fn models_dir() -> Result<PathBuf> {
    ensure(base()?.join("models"))
}

pub fn recordings_dir() -> Result<PathBuf> {
    ensure(base()?.join("recordings"))
}

pub fn session_dir(session_id: &str) -> Result<PathBuf> {
    ensure(recordings_dir()?.join(session_id))
}

/// Note `data_dir()?` rather than `base()?`: both build the same path, but only
/// `data_dir()` guarantees the directory actually exists.
///
/// SQLite does not create parent directories; it simply fails to open the file. On a genuinely
/// fresh install the directory is not there yet, and the app died before a single window
/// appeared. This never showed up during development, because the directory had always
/// already been created by an earlier session.
pub fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("data.db"))
}

pub fn settings_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("settings.json"))
}

/// Log location follows each platform's convention: `~/Library/Logs` on macOS, and the app
/// data directory elsewhere, since Windows has no equivalent.
pub fn log_dir() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let dir = dirs::home_dir()
            .ok_or_else(|| AppError::Io("could not locate the home directory".into()))?
            .join("Library/Logs/auto-transcript");
        ensure(dir)
    }
    #[cfg(not(target_os = "macos"))]
    {
        ensure(base()?.join("logs"))
    }
}
