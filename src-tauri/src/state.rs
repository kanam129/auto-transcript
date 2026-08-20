use crate::error::{AppError, Result};
use crate::session::RecordingSession;
use crate::settings::Settings;
use crate::store::db::Db;
use std::sync::{Arc, Mutex, RwLock};

pub struct AppState {
    pub db: Arc<Db>,
    pub settings: RwLock<Settings>,
    pub session: Mutex<Option<RecordingSession>>,
}

impl AppState {
    pub fn new() -> Result<Self> {
        Ok(Self {
            db: Arc::new(Db::open()?),
            settings: RwLock::new(Settings::load()),
            session: Mutex::new(None),
        })
    }

    pub fn settings_snapshot(&self) -> Result<Settings> {
        self.settings
            .read()
            .map(|s| s.clone())
            .map_err(|_| AppError::Config("settings are locked".into()))
    }

    pub fn update_settings(&self, next: Settings) -> Result<()> {
        next.save()?;
        let mut guard = self
            .settings
            .write()
            .map_err(|_| AppError::Config("settings are locked".into()))?;
        *guard = next;
        Ok(())
    }

    pub fn lock_session(&self) -> Result<std::sync::MutexGuard<'_, Option<RecordingSession>>> {
        self.session
            .lock()
            .map_err(|_| AppError::Busy("session state is locked".into()))
    }
}
