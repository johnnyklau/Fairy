use crate::state::{self, PositionSettings, ScreenCorner};
use serde::Serialize;
use std::time::Duration;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

const SCREEN_EDGE_MARGIN: i32 = 24;
const HOVER_POLL_INTERVAL: Duration = Duration::from_millis(60);

pub fn apply_autostart(app: &AppHandle, enabled: bool) {
    use tauri_plugin_autostart::ManagerExt;

    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    if let Err(err) = result {
        eprintln!("failed to set autostart to {enabled}: {err}");
    }
}

#[derive(serde::Deserialize)]
pub struct EyeBoundsInput {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

/// Renderer calls this after every layout-affecting render (mount, corner
/// change) with `eyeWrap.getBoundingClientRect()`. This is the only source
/// of truth for where the eye actually is — Shell never hardcodes CSS
/// layout values, so a `.eye-wrap` style change alone can't desync hover
/// detection from reality.
#[tauri::command]
pub fn report_eye_bounds(app: AppHandle, bounds: EyeBoundsInput) {
    state::set_eye_bounds(
        &app,
        state::EyeBounds {
            left: bounds.left,
            top: bounds.top,
            width: bounds.width,
            height: bounds.height,
        },
    );
}

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
            let Some(eye) = state::current_eye_bounds(&app) else {
                continue;
            };
            let (Ok(cursor), Ok(win_pos)) = (window.cursor_position(), window.outer_position())
            else {
                continue;
            };
            let scale = window.scale_factor().unwrap_or(1.0);

            let eye_left = win_pos.x as f64 + eye.left * scale;
            let eye_top = win_pos.y as f64 + eye.top * scale;
            let eye_width = eye.width * scale;
            let eye_height = eye.height * scale;

            let inside = cursor.x >= eye_left
                && cursor.x <= eye_left + eye_width
                && cursor.y >= eye_top
                && cursor.y <= eye_top + eye_height;

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
    let window = app
        .get_webview_window("main")
        .ok_or("main window missing")?;
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

/// Pure corner → screen-pixel math, isolated from the live `Window`/
/// `Monitor` API so it can be unit tested directly — including multi-monitor
/// setups where a monitor's origin can be negative (e.g. a monitor placed
/// to the left of or above the primary one).
fn compute_window_position(
    corner: ScreenCorner,
    monitor_pos: (i32, i32),
    monitor_size: (u32, u32),
    window_size: (u32, u32),
    margin: i32,
) -> (i32, i32) {
    let (mon_x, mon_y) = monitor_pos;
    let (mon_w, mon_h) = (monitor_size.0 as i32, monitor_size.1 as i32);
    let (win_w, win_h) = (window_size.0 as i32, window_size.1 as i32);

    match corner {
        ScreenCorner::TopLeft => (mon_x + margin, mon_y + margin),
        ScreenCorner::TopRight => (mon_x + mon_w - win_w - margin, mon_y + margin),
        ScreenCorner::BottomLeft => (mon_x + margin, mon_y + mon_h - win_h - margin),
        ScreenCorner::BottomRight => (
            mon_x + mon_w - win_w - margin,
            mon_y + mon_h - win_h - margin,
        ),
    }
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

    let (x, y) = compute_window_position(
        position.corner,
        (mon_pos.x, mon_pos.y),
        (mon_size.width, mon_size.height),
        (win_size.width, win_size.height),
        SCREEN_EDGE_MARGIN,
    );

    let _ = window.set_position(PhysicalPosition::new(x, y));
    state::set_window_corner(app, position.corner);
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: (u32, u32) = (460, 220);
    const MARGIN: i32 = 24;

    #[test]
    fn top_left_hugs_the_origin_corner() {
        let (x, y) =
            compute_window_position(ScreenCorner::TopLeft, (0, 0), (1920, 1080), WINDOW, MARGIN);
        assert_eq!((x, y), (24, 24));
    }

    #[test]
    fn top_right_accounts_for_window_width() {
        let (x, y) =
            compute_window_position(ScreenCorner::TopRight, (0, 0), (1920, 1080), WINDOW, MARGIN);
        assert_eq!((x, y), (1920 - 460 - 24, 24));
    }

    #[test]
    fn bottom_left_accounts_for_window_height() {
        let (x, y) = compute_window_position(
            ScreenCorner::BottomLeft,
            (0, 0),
            (1920, 1080),
            WINDOW,
            MARGIN,
        );
        assert_eq!((x, y), (24, 1080 - 220 - 24));
    }

    #[test]
    fn bottom_right_accounts_for_both_dimensions() {
        let (x, y) = compute_window_position(
            ScreenCorner::BottomRight,
            (0, 0),
            (1920, 1080),
            WINDOW,
            MARGIN,
        );
        assert_eq!((x, y), (1920 - 460 - 24, 1080 - 220 - 24));
    }

    #[test]
    fn secondary_monitor_with_negative_origin() {
        // A monitor placed to the left of and above the primary monitor
        // (e.g. Windows' "arrange displays" with a monitor at top-left of
        // a multi-monitor layout) has a negative origin. The math must
        // stay relative to that monitor's own origin, not (0, 0).
        let (x, y) = compute_window_position(
            ScreenCorner::TopRight,
            (-1920, -200),
            (1920, 1080),
            WINDOW,
            MARGIN,
        );
        assert_eq!((x, y), (-1920 + 1920 - 460 - 24, -200 + 24));
    }

    #[test]
    fn secondary_monitor_bottom_right_with_negative_origin() {
        let (x, y) = compute_window_position(
            ScreenCorner::BottomRight,
            (-1920, -200),
            (1920, 1080),
            WINDOW,
            MARGIN,
        );
        assert_eq!((x, y), (-1920 + 1920 - 460 - 24, -200 + 1080 - 220 - 24));
    }

    #[test]
    fn zero_margin_hugs_the_exact_edge() {
        let (x, y) =
            compute_window_position(ScreenCorner::TopLeft, (0, 0), (1920, 1080), WINDOW, 0);
        assert_eq!((x, y), (0, 0));
    }
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
