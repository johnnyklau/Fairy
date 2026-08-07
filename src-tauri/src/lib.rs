mod behavior;
mod settings;
mod shell;
mod state;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_focus();
        }
    }));

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::update_settings,
            shell::set_click_through,
            shell::close_settings,
            shell::list_monitors,
            state::get_state,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.set_ignore_cursor_events(true);
            }
            let settings = settings::load_settings(&handle);
            shell::apply_window_position(&handle, &settings.position);
            shell::setup_tray(&handle)?;
            shell::start_hover_watcher(handle.clone());
            behavior::start_scheduler(handle.clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
