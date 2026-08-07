use crate::state::{self, PositionSettings, ScreenCorner};
use serde::Serialize;
use std::time::Duration;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

const SCREEN_EDGE_MARGIN: i32 = 24;

// Local (CSS-pixel) bounding box of the eye within the main window. Mirrors
// `.eye-wrap` / `.companion.anchor-right` in styles.css: the eye sits at the
// left edge normally, or the right edge when the window is anchored to a
// right-side screen corner.
const EYE_LOCAL_TOP: f64 = 50.0;
const EYE_LOCAL_SIZE: f64 = 120.0;
const EYE_LOCAL_LEFT_WHEN_LEFT_ANCHORED: f64 = 10.0;
const EYE_LOCAL_LEFT_WHEN_RIGHT_ANCHORED: f64 = 460.0 - 10.0 - EYE_LOCAL_SIZE;
const HOVER_POLL_INTERVAL: Duration = Duration::from_millis(60);

// `set_ignore_cursor_events(true)` blocks ALL mouse input to the webview,
// including the mouseenter event that would otherwise ask to turn it back
// off — so hover detection can't live in JS/Renderer. Shell polls the
// OS-level global cursor position instead, which works even while the
// window is ignoring cursor events.
pub fn start_hover_watcher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut hovering = false;
        loop {
            tokio::time::sleep(HOVER_POLL_INTERVAL).await;

            let Some(window) = app.get_webview_window("main") else {
                continue;
            };
            let (Ok(cursor), Ok(win_pos)) = (window.cursor_position(), window.outer_position())
            else {
                continue;
            };
            let scale = window.scale_factor().unwrap_or(1.0);

            let anchor_right = matches!(
                state::current_corner(&app),
                ScreenCorner::TopRight | ScreenCorner::BottomRight
            );
            let eye_local_left = if anchor_right {
                EYE_LOCAL_LEFT_WHEN_RIGHT_ANCHORED
            } else {
                EYE_LOCAL_LEFT_WHEN_LEFT_ANCHORED
            };

            let eye_left = win_pos.x as f64 + eye_local_left * scale;
            let eye_top = win_pos.y as f64 + EYE_LOCAL_TOP * scale;
            let eye_size = EYE_LOCAL_SIZE * scale;

            let inside = cursor.x >= eye_left
                && cursor.x <= eye_left + eye_size
                && cursor.y >= eye_top
                && cursor.y <= eye_top + eye_size;

            if inside != hovering {
                hovering = inside;
                let _ = window.set_ignore_cursor_events(!hovering);
                state::set_click_through(&app, !hovering);
            }
        }
    });
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub index: usize,
    pub name: Option<String>,
    pub width: u32,
    pub height: u32,
}

#[tauri::command]
pub fn list_monitors(app: AppHandle) -> Result<Vec<MonitorInfo>, String> {
    let window = app.get_webview_window("main").ok_or("main window missing")?;
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    Ok(monitors
        .iter()
        .enumerate()
        .map(|(index, monitor)| MonitorInfo {
            index,
            name: monitor.name().cloned(),
            width: monitor.size().width,
            height: monitor.size().height,
        })
        .collect())
}

pub fn apply_window_position(app: &AppHandle, position: &PositionSettings) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let Ok(monitors) = window.available_monitors() else {
        return;
    };
    let Some(monitor) = monitors
        .get(position.monitor_index as usize)
        .or_else(|| monitors.first())
    else {
        return;
    };
    let Ok(win_size) = window.outer_size() else {
        return;
    };

    let mon_pos = monitor.position();
    let mon_size = monitor.size();

    let (x, y) = match position.corner {
        ScreenCorner::TopLeft => (mon_pos.x + SCREEN_EDGE_MARGIN, mon_pos.y + SCREEN_EDGE_MARGIN),
        ScreenCorner::TopRight => (
            mon_pos.x + mon_size.width as i32 - win_size.width as i32 - SCREEN_EDGE_MARGIN,
            mon_pos.y + SCREEN_EDGE_MARGIN,
        ),
        ScreenCorner::BottomLeft => (
            mon_pos.x + SCREEN_EDGE_MARGIN,
            mon_pos.y + mon_size.height as i32 - win_size.height as i32 - SCREEN_EDGE_MARGIN,
        ),
        ScreenCorner::BottomRight => (
            mon_pos.x + mon_size.width as i32 - win_size.width as i32 - SCREEN_EDGE_MARGIN,
            mon_pos.y + mon_size.height as i32 - win_size.height as i32 - SCREEN_EDGE_MARGIN,
        ),
    };

    let _ = window.set_position(PhysicalPosition::new(x, y));
    state::set_window_corner(app, position.corner);
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open_item = MenuItemBuilder::with_id("open_settings", "Open Settings").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&open_item, &quit_item])
        .build()?;

    let app_handle = app.clone();
    TrayIconBuilder::new()
        .menu(&menu)
        .icon(
            app.default_window_icon()
                .cloned()
                .expect("app icon must be configured for the tray"),
        )
        .on_menu_event(move |_app, event| match event.id.as_ref() {
            "open_settings" => {
                let _ = open_settings_window(&app_handle);
            }
            "quit" => app_handle.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn open_settings_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("settings") {
        window.show()?;
        window.set_focus()?;
    } else {
        let window =
            WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
                .title("Fairy Settings")
                .inner_size(360.0, 560.0)
                .resizable(false)
                .build()?;

        let app_handle = app.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Some(w) = app_handle.get_webview_window("settings") {
                    let _ = w.hide();
                }
                state::set_settings_open(&app_handle, false);
            }
        });
    }
    state::set_settings_open(app, true);
    Ok(())
}

#[tauri::command]
pub fn close_settings(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        window.hide().map_err(|e| e.to_string())?;
    }
    state::set_settings_open(&app, false);
    Ok(())
}

#[tauri::command]
pub fn set_click_through(app: AppHandle, click_through: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_ignore_cursor_events(click_through)
            .map_err(|e| e.to_string())?;
    }
    state::set_click_through(&app, click_through);
    Ok(())
}
