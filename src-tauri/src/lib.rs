pub mod audio;
pub mod commands;
pub mod error;
pub mod events;
pub mod paths;
pub mod retranscribe;
pub mod session;
pub mod settings;
pub mod state;
pub mod stt;
pub mod store;
pub mod summarize;

use state::AppState;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

/// The keystroke that starts and stops recording while the app is in the background.
///
/// Command on macOS, Control everywhere else. `SUPER` on Windows is the Windows key, and
/// Win+Shift+R already belongs to the system screen recorder.
fn toggle_shortcut() -> Shortcut {
    #[cfg(target_os = "macos")]
    let mods = Modifiers::SUPER | Modifiers::SHIFT;
    #[cfg(not(target_os = "macos"))]
    let mods = Modifiers::CONTROL | Modifiers::SHIFT;
    Shortcut::new(Some(mods), Code::KeyR)
}

fn init_logging() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let filter = EnvFilter::try_from_env("AUTO_TRANSCRIPT_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,auto_transcript_lib=debug"));

    let file_layer = paths::log_dir().ok().map(|dir| {
        let appender = tracing_appender::rolling::daily(dir, "app.log");
        fmt::layer().with_ansi(false).with_writer(appender)
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(file_layer)
        .init();
}

/// Sessions that were never closed — because the app died — get an end time derived from the
/// last segment that made it to disk, so history does not show a recording that runs
/// forever.
fn recover_unfinished(state: &AppState) {
    let Ok(list) = state.db.unfinished_sessions() else {
        return;
    };
    for s in list {
        let end = state
            .db
            .get_segments(&s.id, &[])
            .ok()
            .and_then(|segs| segs.last().map(|x| x.end_ms))
            .unwrap_or(0);
        if let Err(e) = state.db.finish_session(&s.id, s.started_at + end, end) {
            tracing::warn!("could not recover session {}: {e}", s.id);
        } else {
            tracing::info!("recovered session {} after an unclean shutdown", s.id);
        }
    }
}

/// Reports a fatal startup failure, then exits.
///
/// Before this existed, a failure inside Tauri's setup hook panicked inside an FFI callback
/// that cannot unwind, so the process aborted instantly. From the user's side the icon
/// bounced once and nothing happened: no window, no message.
///
/// The message box is deliberately a separate process rather than the dialog plugin: at this
/// point the app's run loop may not be running, and a blocking dialog on the main thread
/// risks hanging forever.
fn fatal_startup_error(message: &str) -> ! {
    tracing::error!("startup failed: {message}");
    eprintln!("Auto-Transcript could not start: {message}");

    let safe = message.replace('"', "'").replace('\\', "/");
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "display alert \"Auto-Transcript could not start\" message \"{safe}\" as critical"
        ))
        .status();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Add-Type -AssemblyName PresentationFramework; \
                 [System.Windows.MessageBox]::Show('{safe}','Auto-Transcript could not start')"
            ),
        ])
        .status();

    std::process::exit(1)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            // A global shortcut works while the app is in the background, so recording can
            // be started without leaving the meeting window.
            //
            // Registered here rather than through the plugin builder on purpose: when the
            // combination already belongs to another application, that must cost us the
            // shortcut and nothing more. Registering it at plugin-init time turned a taken
            // hotkey into a panic before any window appeared — which is exactly what
            // happened on Windows, where Win+Shift+R is the system screen recorder.
            if let Err(e) = app.global_shortcut().on_shortcut(
                toggle_shortcut(),
                move |app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        if let Err(e) = app.emit("shortcut:toggle", ()) {
                            tracing::warn!("could not emit shortcut event: {e}");
                        }
                    }
                },
            ) {
                tracing::warn!(
                    "the global start/stop shortcut is unavailable, most likely because \
                     another application owns it: {e}"
                );
            }

            let state = match AppState::new() {
                Ok(s) => s,
                Err(e) => fatal_startup_error(&e.to_string()),
            };
            recover_unfinished(&state);

            // Guess the audio sources on first run so nobody has to configure anything
            // before they can record.
            if let Ok(mut s) = state.settings.write() {
                if s.system_source_id.is_none() {
                    if let Ok(sources) = audio::devices::list_sources() {
                        let (sys, mic) = audio::devices::guess_defaults(&sources);
                        s.system_source_id = sys;
                        s.mic_source_id = mic;
                        let _ = s.save();
                    }
                }
            }

            if let Some(w) = app.get_webview_window("main") {
                // Asserted at runtime rather than only in tauri.conf.json: the window-state
                // plugin restores a saved size and position, and if that file came from an
                // older version its attributes can come along with it.
                let _ = w.set_resizable(true);
                if let Ok(s) = state.settings_snapshot() {
                    if s.always_on_top {
                        let _ = w.set_always_on_top(true);
                    }
                }
            }

            app.manage(state);

            // Cleaning up old recordings runs in the background: with a large recordings
            // folder this must not delay the window appearing.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                use tauri::Manager as _;
                let state = handle.state::<AppState>();
                match commands::cleanup_old_audio(state) {
                    Ok(0) => {}
                    Ok(freed) => tracing::info!("cleaned up {freed} bytes of old recordings"),
                    Err(e) => tracing::warn!("cleanup of old recordings failed: {e}"),
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_audio_sources,
            commands::get_settings,
            commands::set_settings,
            commands::start_session,
            commands::stop_session,
            commands::get_recording_state,
            commands::list_sessions,
            commands::get_session,
            commands::rename_session,
            commands::edit_segment,
            commands::delete_session,
            commands::export_session,
            commands::disk_usage,
            commands::cleanup_old_audio,
            commands::reveal_in_finder,
            commands::app_platform,
            commands::list_models,
            commands::list_profiles,
            commands::apply_profile,
            commands::data_paths,
            commands::download_model,
            commands::delete_model,
            commands::transcribe_mic,
            commands::set_llm_api_key,
            commands::has_llm_api_key,
            commands::get_summary,
            commands::summarize_session,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run the application");
}
