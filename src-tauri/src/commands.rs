use crate::audio::{devices, Track};
use crate::error::{AppError, Result};
use crate::events;
use crate::paths;
use crate::session::{self, RecordingState, StartOptions};
use crate::settings::Settings;
use crate::state::AppState;
use crate::stt::model_manager;
use crate::store::db::{Segment, SessionMeta};
use crate::store::export;
use crate::summarize;
use serde::Serialize;
use std::time::Instant;
use tauri::{AppHandle, State};

// ---------------------------------------------------------------- audio & settings

#[tauri::command]
pub fn list_audio_sources() -> Result<Vec<crate::audio::SourceInfo>> {
    devices::list_sources()
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    state.settings_snapshot()
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    state.update_settings(settings)?;
    state.settings_snapshot()
}

// ---------------------------------------------------------------- recording sessions

#[tauri::command]
pub fn start_session(
    app: AppHandle,
    state: State<'_, AppState>,
    title: Option<String>,
    system_source_id: Option<String>,
    mic_source_id: Option<String>,
) -> Result<RecordingState> {
    let mut guard = state.lock_session()?;
    if guard.is_some() {
        return Err(AppError::Busy("a recording session is already running".into()));
    }
    let settings = state.settings_snapshot()?;
    let sess = session::start(
        app.clone(),
        state.db.clone(),
        &settings,
        StartOptions {
            title,
            system_source_id,
            mic_source_id,
        },
    )?;
    let st = sess.state();
    *guard = Some(sess);
    events::emit(&app, events::SESSION_STATE, st.clone());
    Ok(st)
}

#[tauri::command]
pub fn stop_session(app: AppHandle, state: State<'_, AppState>) -> Result<SessionMeta> {
    let sess = {
        let mut guard = state.lock_session()?;
        guard.take().ok_or_else(|| AppError::NotFound("an active session".into()))?
    };
    let meta = session::stop(sess, &state.db)?;
    events::emit(
        &app,
        events::SESSION_STATE,
        RecordingState {
            recording: false,
            session_id: None,
            title: None,
            elapsed_ms: 0,
            model_id: None,
            source_name: None,
            queue_depth: 0,
        },
    );
    Ok(meta)
}

#[tauri::command]
pub fn get_recording_state(state: State<'_, AppState>) -> Result<RecordingState> {
    let guard = state.lock_session()?;
    Ok(match guard.as_ref() {
        Some(s) => s.state(),
        None => RecordingState {
            recording: false,
            session_id: None,
            title: None,
            elapsed_ms: 0,
            model_id: None,
            source_name: None,
            queue_depth: 0,
        },
    })
}

// ---------------------------------------------------------------- history

#[tauri::command]
pub fn list_sessions(
    state: State<'_, AppState>,
    q: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<SessionMeta>> {
    state
        .db
        .list_sessions(q.as_deref(), limit.unwrap_or(50), offset.unwrap_or(0))
}

#[derive(Serialize)]
pub struct FullSession {
    pub meta: SessionMeta,
    pub segments: Vec<Segment>,
}

#[tauri::command]
pub fn get_session(
    state: State<'_, AppState>,
    id: String,
    include_mic: Option<bool>,
) -> Result<FullSession> {
    let meta = state.db.get_session(&id)?;
    let tracks: Vec<Track> = if include_mic.unwrap_or(false) {
        vec![]
    } else {
        vec![Track::System]
    };
    let segments = state.db.get_segments(&id, &tracks)?;
    Ok(FullSession { meta, segments })
}

#[tauri::command]
pub fn rename_session(state: State<'_, AppState>, id: String, title: String) -> Result<()> {
    state.db.rename_session(&id, title.trim())
}

#[tauri::command]
pub fn edit_segment(state: State<'_, AppState>, id: i64, text: String) -> Result<()> {
    state.db.edit_segment(id, text.trim())
}

#[tauri::command]
pub fn delete_session(
    state: State<'_, AppState>,
    id: String,
    delete_audio: Option<bool>,
) -> Result<()> {
    if delete_audio.unwrap_or(true) {
        if let Ok(dir) = paths::recordings_dir() {
            let target = dir.join(&id);
            if target.exists() {
                if let Err(e) = std::fs::remove_dir_all(&target) {
                    tracing::warn!("could not delete recording {}: {e}", target.display());
                }
            }
        }
    }
    state.db.delete_session(&id)
}

#[tauri::command]
pub fn export_session(
    state: State<'_, AppState>,
    id: String,
    format: String,
    path: String,
    include_mic: Option<bool>,
) -> Result<String> {
    let meta = state.db.get_session(&id)?;
    let tracks: Vec<Track> = if include_mic.unwrap_or(true) {
        vec![]
    } else {
        vec![Track::System]
    };
    let segments = state.db.get_segments(&id, &tracks)?;
    let body = export::render(&meta, &segments, &format)?;
    std::fs::write(&path, body)?;
    Ok(path)
}

#[derive(Serialize)]
pub struct DiskUsage {
    pub bytes: u64,
    pub sessions: usize,
}

#[tauri::command]
pub fn disk_usage() -> Result<DiskUsage> {
    let dir = paths::recordings_dir()?;
    let mut bytes = 0u64;
    let mut sessions = 0usize;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if e.path().is_dir() {
                sessions += 1;
                if let Ok(files) = std::fs::read_dir(e.path()) {
                    for f in files.flatten() {
                        bytes += f.metadata().map(|m| m.len()).unwrap_or(0);
                    }
                }
            }
        }
    }
    Ok(DiskUsage { bytes, sessions })
}

/// Deletes audio files older than `keep_audio_days`. Transcripts are kept — only the raw
/// recordings go.
#[tauri::command]
pub fn cleanup_old_audio(state: State<'_, AppState>) -> Result<u64> {
    let settings = state.settings_snapshot()?;
    let Some(days) = settings.keep_audio_days else {
        return Ok(0);
    };
    let cutoff = session::now_ms() - (days as i64) * 86_400_000;
    let mut freed = 0u64;
    for s in state.db.list_sessions(None, 10_000, 0)? {
        if s.started_at >= cutoff {
            continue;
        }
        let dir = paths::recordings_dir()?.join(&s.id);
        if !dir.exists() {
            continue;
        }
        if let Ok(files) = std::fs::read_dir(&dir) {
            for f in files.flatten() {
                freed += f.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    Ok(freed)
}

/// Reveals a file in the platform's file manager.
#[tauri::command]
pub fn reveal_in_finder(path: String) -> Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(&path);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("explorer");
        // explorer.exe demands exactly this form: no space after the comma.
        c.arg(format!("/select,{path}"));
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(
            std::path::Path::new(&path)
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf(),
        );
        c
    };

    cmd.spawn()
        .map_err(|e| AppError::Io(format!("could not open the file manager: {e}")))?;
    Ok(())
}

/// Used by the frontend to adjust layout — for example the macOS traffic-light gutter, which
/// does not exist on Windows.
#[tauri::command]
pub fn app_platform() -> &'static str {
    std::env::consts::OS
}

// ---------------------------------------------------------------- models

#[tauri::command]
pub fn list_models() -> Result<Vec<model_manager::ModelStatus>> {
    model_manager::status_all()
}

#[tauri::command]
pub fn list_profiles() -> Result<Vec<model_manager::ProfileStatus>> {
    model_manager::profile_status()
}

/// Applies one quality profile, setting the main and preview model together. Downloading is
/// left to the frontend via `download_model` so progress stays visible.
#[tauri::command]
pub fn apply_profile(state: State<'_, AppState>, profile_id: String) -> Result<Settings> {
    let profile = model_manager::profiles()
        .into_iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| AppError::NotFound(format!("quality profile '{profile_id}'")))?;

    let mut settings = state.settings_snapshot()?;
    settings.model_id = profile.model_id.to_string();
    settings.partial_model_id = profile.partial_model_id.to_string();
    state.update_settings(settings)?;
    state.settings_snapshot()
}

/// Every location the app stores data in, shown in Settings so people know exactly what is
/// kept on their machine and where.
#[tauri::command]
pub fn data_paths() -> Result<DataPaths> {
    Ok(DataPaths {
        data_dir: paths::data_dir()?.to_string_lossy().to_string(),
        models_dir: paths::models_dir()?.to_string_lossy().to_string(),
        recordings_dir: paths::recordings_dir()?.to_string_lossy().to_string(),
        database: paths::db_path()?.to_string_lossy().to_string(),
        log_dir: paths::log_dir()?.to_string_lossy().to_string(),
    })
}

#[derive(Serialize)]
pub struct DataPaths {
    pub data_dir: String,
    pub models_dir: String,
    pub recordings_dir: String,
    pub database: String,
    pub log_dir: String,
}

#[tauri::command]
pub async fn download_model(app: AppHandle, model_id: String) -> Result<String> {
    let id_for_events = model_id.clone();
    let mut last = Instant::now();
    let path = model_manager::download(&model_id, move |downloaded, total| {
        // Tell the frontend at most five times a second; more than that just floods IPC.
        if last.elapsed().as_millis() >= 200 || downloaded >= total {
            last = Instant::now();
            events::emit(
                &app,
                events::MODEL_DOWNLOAD,
                events::DownloadEvent {
                    model_id: id_for_events.clone(),
                    downloaded,
                    total,
                },
            );
        }
    })
    .await?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn delete_model(state: State<'_, AppState>, model_id: String) -> Result<()> {
    let settings = state.settings_snapshot()?;
    if settings.model_id == model_id || settings.partial_model_id == model_id {
        return Err(AppError::Config("this model is in use; pick a different one first".into()));
    }
    model_manager::delete(&model_id)
}

// ---------------------------------------------------------------- microphone re-transcription

/// Re-transcribes the microphone track from its stored WAV. Useful when the microphone fell
/// behind during a meeting, or when the model has since changed.
#[tauri::command]
pub fn transcribe_mic(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let meta = state.db.get_session(&id)?;
    let Some(path) = meta.mic_wav_path.clone() else {
        return Err(AppError::NotFound("a microphone recording for this session".into()));
    };
    let path = std::path::PathBuf::from(path);
    if !path.exists() {
        return Err(AppError::NotFound(format!("file {}", path.display())));
    }
    let settings = state.settings_snapshot()?;
    let db = state.db.clone();

    std::thread::Builder::new()
        .name("mic-retranscribe".into())
        .spawn(move || {
            if let Err(e) = crate::retranscribe::run(&app, &db, &id, &path, &settings) {
                tracing::error!("microphone re-transcription failed: {e}");
                events::emit(
                    &app,
                    events::AUDIO_ERROR,
                    events::ErrorEvent {
                        code: "retranscribe",
                        message: e.to_string(),
                    },
                );
            }
        })
        .map_err(|e| AppError::Io(format!("could not spawn thread: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------- AI summaries

#[tauri::command]
pub fn set_llm_api_key(key: String) -> Result<()> {
    if key.trim().is_empty() {
        summarize::llm::clear_api_key()
    } else {
        summarize::llm::store_api_key(key.trim())
    }
}

#[tauri::command]
pub fn has_llm_api_key() -> bool {
    summarize::llm::has_api_key()
}

#[tauri::command]
pub fn get_summary(state: State<'_, AppState>, id: String) -> Result<Option<String>> {
    state.db.get_summary(&id)
}

/// Fully functional, but deliberately gated behind `summary_enabled`, which defaults to off
/// and has no button in the UI yet.
#[tauri::command]
pub async fn summarize_session(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<String> {
    let settings = state.settings_snapshot()?;
    if !settings.summary_enabled {
        return Err(AppError::Disabled("AI summaries are not enabled in settings".into()));
    }
    let meta = state.db.get_session(&id)?;
    let segments = state.db.get_segments(&id, &[])?;
    if segments.is_empty() {
        return Err(AppError::Config("this session has no transcript yet".into()));
    }

    let lines: Vec<summarize::TranscriptLine> = segments
        .iter()
        .map(|s| summarize::TranscriptLine {
            at_ms: s.start_ms,
            speaker: export::speaker_label(&s.track).to_string(),
            text: s.text.clone(),
        })
        .collect();

    let client = summarize::llm::OpenAiCompatible::new(settings.llm.clone())?;
    let app_for_progress = app.clone();
    let id_for_progress = id.clone();

    use summarize::Summarizer;
    let summary = client
        .summarize(
            summarize::SummaryRequest {
                title: meta.title.clone(),
                lines,
            },
            move |stage, done, total| {
                events::emit(
                    &app_for_progress,
                    events::SUMMARY_PROGRESS,
                    events::SummaryProgressEvent {
                        session_id: id_for_progress.clone(),
                        stage: stage.to_string(),
                        done,
                        total,
                    },
                );
            },
        )
        .await?;

    let json = serde_json::to_string(&summary)?;
    state.db.save_summary(
        &id,
        "openai-compatible",
        &settings.llm.model,
        &json,
        session::now_ms(),
    )?;
    events::emit(&app, events::SUMMARY_READY, id.clone());
    Ok(json)
}
