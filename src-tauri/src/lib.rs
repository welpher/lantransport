mod commands;
pub mod server;
mod settings;
mod state;
mod tray;

use state::AppState;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(AppState::new(&handle));
            tray::setup(&handle)?;
            let auto_start = {
                let state = handle.state::<AppState>();
                let settings = state
                    .settings
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                settings.auto_start && !settings.shares.is_empty()
            };
            if auto_start {
                let _ = commands::start_from_settings(&handle);
            }
            commands::notify(&handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::DragDrop(drag) = event {
                if let tauri::DragDropEvent::Drop { paths, .. } = drag {
                    let (added, known) = commands::add_shared_folders(window.app_handle(), paths);
                    let _ = window.app_handle().emit("drop-result", (added, known));
                }
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let minimize = {
                    let state = window.app_handle().state::<AppState>();
                    let settings = state
                        .settings
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    settings.minimize_to_tray
                };
                if minimize {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::start_sharing,
            commands::stop_sharing,
            commands::update_prefs,
            commands::update_upload,
            commands::update_password,
            commands::check_port,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            tauri::RunEvent::Exit => commands::shutdown(app),
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => tray::show_main(app),
            _ => {}
        });
}
