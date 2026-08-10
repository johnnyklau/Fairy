mod behavior;
mod dialogues;
mod settings;
mod shell;
mod state;
mod voice;

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
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::update_settings,
            shell::close_settings,
            shell::list_monitors,
            shell::report_eye_bounds,
            shell::set_flavor_popup_visible,
            state::get_state,
            voice::synthesize_flavor_line,
            voice::get_flavor_lines,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.set_ignore_cursor_events(true);
            }
            let settings = settings::load_settings(&handle);
            shell::apply_window_position(&handle, &settings.position);
            shell::apply_autostart(&handle, settings.autostart.enabled);
            shell::setup_tray(&handle)?;
            shell::start_hover_watcher(handle.clone());
            behavior::start_scheduler(handle.clone());
            // Eager background model load/download so the first reminder
            // of a session isn't delayed by a multi-second cold model load
            // stacking on top of per-call inference time (see
            // VOICE_SPEC.md "Synthesis pipeline & caching").
            voice::maybe_start_model_download(&handle, &settings.voice);
            voice::maybe_eager_load(&handle, &settings.voice);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
