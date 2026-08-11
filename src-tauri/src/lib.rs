mod behavior;
mod dialogues;
mod settings;
mod shell;
mod state;
mod voice;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Reminders' audio is triggered by a backend IPC event, not a click,
    // so Chromium's autoplay policy (inherited by WebView2) silently
    // blocks it — set here, as a process-wide env var, before any WebView2
    // environment initializes. Deliberately NOT the per-window
    // `additionalBrowserArgs` in tauri.conf.json: that applies only to the
    // window declared there ("main"), and requesting a *different*
    // browser-args configuration for a second window created later (the
    // Settings window, built dynamically) causes a WebView2 environment
    // conflict — Settings would appear to open, then its native window
    // silently got torn down with no close/destroy event ever reaching
    // Tauri. This env var applies uniformly to every WebView2 instance in
    // the process, so all windows agree on the same environment config.
    #[cfg(target_os = "windows")]
    // SAFETY: called once at process startup, before any other thread
    // exists (no plugins/windows/async runtime started yet) — no
    // concurrent env access is possible at this point.
    unsafe {
        std::env::set_var(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--autoplay-policy=no-user-gesture-required",
        );
    }

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
