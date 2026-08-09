pub mod commands;
pub mod error;
pub mod persistence;
pub mod presets;
pub mod protocol;
pub mod session;
pub mod worktree;

use session::manager::SessionManager;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,lighter=debug".into()),
        )
        .init();

    tauri::Builder::default()
        // Must be registered first: a second launch focuses the existing window
        // instead of spawning a competing instance (which would race over state
        // files and duplicate claude processes).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(SessionManager::default())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            app.manage(presets::Presets::load(persistence::Store::new(dir)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::create_session,
            commands::attach_session,
            commands::send_user_message,
            commands::respond_permission,
            commands::set_permission_mode,
            commands::set_model,
            commands::interrupt_session,
            commands::stop_session,
            commands::remove_session,
            commands::list_sessions,
            commands::set_focus,
            commands::attach_registry,
            commands::list_presets,
            commands::save_preset,
            commands::delete_preset,
        ])
        .on_window_event(|window, event| {
            // Closing the window gracefully stops every session first so CLIs
            // can flush their transcripts (resume depends on it). Job objects
            // still guarantee cleanup if anything survives.
            if let tauri::WindowEvent::Destroyed = event {
                if window.label() == "main" {
                    let manager = window.state::<SessionManager>();
                    manager.stop_all();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
