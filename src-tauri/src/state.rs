use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Idle,
    Reminder,
    SettingsOpen,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReminderType {
    Water,
    Break,
    Workout,
    IdleBark,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GlowIntensity {
    Low,
    High,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ActiveReminder {
    #[serde(rename = "type")]
    pub kind: ReminderType,
    pub message: String,
    pub triggered_at: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EyeState {
    glow_intensity: GlowIntensity,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowState {
    click_through: bool,
    corner: ScreenCorner,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionState {
    mode: Mode,
    active_reminder: Option<ActiveReminder>,
    eye: EyeState,
    window: WindowState,
}

impl Default for CompanionState {
    fn default() -> Self {
        Self {
            mode: Mode::Idle,
            active_reminder: None,
            eye: EyeState {
                glow_intensity: GlowIntensity::Low,
            },
            window: WindowState {
                click_through: true,
                corner: ScreenCorner::TopRight,
            },
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WaterSettings {
    pub enabled: bool,
    pub interval_minutes: u32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BreakSettings {
    pub enabled: bool,
    pub interval_minutes: u32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkoutSettings {
    pub enabled: bool,
    pub time_of_day: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IdleBarkSettings {
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ScreenCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PositionSettings {
    pub corner: ScreenCorner,
    pub monitor_index: u32,
}

impl Default for PositionSettings {
    fn default() -> Self {
        Self {
            corner: ScreenCorner::TopRight,
            monitor_index: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutostartSettings {
    pub enabled: bool,
}

impl Default for AutostartSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub water: WaterSettings,
    pub break_reminder: BreakSettings,
    pub workout: WorkoutSettings,
    pub idle_bark: IdleBarkSettings,
    #[serde(default)]
    pub position: PositionSettings,
    #[serde(default)]
    pub autostart: AutostartSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            water: WaterSettings {
                enabled: true,
                interval_minutes: 60,
            },
            break_reminder: BreakSettings {
                enabled: true,
                interval_minutes: 50,
            },
            workout: WorkoutSettings {
                enabled: true,
                time_of_day: "18:00".into(),
            },
            idle_bark: IdleBarkSettings { enabled: false },
            position: PositionSettings {
                corner: ScreenCorner::TopRight,
                monitor_index: 0,
            },
            autostart: AutostartSettings { enabled: true },
        }
    }
}

/// The eye's real, rendered bounding box (CSS logical px, window-relative),
/// as reported by the Renderer via `report_eye_bounds`. This is the single
/// source of truth for hover/click-through hit-testing — Shell never
/// hardcodes CSS layout values, it just trusts what the DOM actually
/// measured, so a CSS change alone can never desync it.
#[derive(Clone, Copy)]
pub struct EyeBounds {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

pub struct AppState {
    pub companion: Mutex<CompanionState>,
    pub eye_bounds: Mutex<Option<EyeBounds>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            companion: Mutex::new(CompanionState::default()),
            eye_bounds: Mutex::new(None),
        }
    }
}

fn emit_state_changed(app: &AppHandle, state: &CompanionState) {
    let _ = app.emit("state_changed", state.clone());
}

pub fn set_mode_idle(app: &AppHandle) {
    let state_handle = app.state::<AppState>();
    let mut companion = state_handle.companion.lock().unwrap();
    companion.mode = Mode::Idle;
    companion.active_reminder = None;
    companion.eye.glow_intensity = GlowIntensity::Low;
    emit_state_changed(app, &companion);
}

pub fn set_reminder(app: &AppHandle, kind: ReminderType, message: String, triggered_at: i64) {
    let state_handle = app.state::<AppState>();
    let mut companion = state_handle.companion.lock().unwrap();
    companion.mode = Mode::Reminder;
    companion.active_reminder = Some(ActiveReminder {
        kind,
        message,
        triggered_at,
    });
    companion.eye.glow_intensity = GlowIntensity::High;
    emit_state_changed(app, &companion);
}

pub fn set_settings_open(app: &AppHandle, open: bool) {
    let state_handle = app.state::<AppState>();
    let mut companion = state_handle.companion.lock().unwrap();
    companion.mode = if open { Mode::SettingsOpen } else { Mode::Idle };
    if !open {
        companion.active_reminder = None;
        companion.eye.glow_intensity = GlowIntensity::Low;
    }
    emit_state_changed(app, &companion);
}

pub fn set_click_through(app: &AppHandle, click_through: bool) {
    let state_handle = app.state::<AppState>();
    let mut companion = state_handle.companion.lock().unwrap();
    companion.window.click_through = click_through;
    emit_state_changed(app, &companion);
}

pub fn set_window_corner(app: &AppHandle, corner: ScreenCorner) {
    let state_handle = app.state::<AppState>();
    let mut companion = state_handle.companion.lock().unwrap();
    companion.window.corner = corner;
    emit_state_changed(app, &companion);
}

pub fn set_eye_bounds(app: &AppHandle, bounds: EyeBounds) {
    let state_handle = app.state::<AppState>();
    *state_handle.eye_bounds.lock().unwrap() = Some(bounds);
}

pub fn current_eye_bounds(app: &AppHandle) -> Option<EyeBounds> {
    let state_handle = app.state::<AppState>();
    let bounds = state_handle.eye_bounds.lock().unwrap();
    *bounds
}

pub fn current_mode(app: &AppHandle) -> Mode {
    let state_handle = app.state::<AppState>();
    let companion = state_handle.companion.lock().unwrap();
    companion.mode
}

#[tauri::command]
pub fn get_state(app: AppHandle) -> CompanionState {
    let state_handle = app.state::<AppState>();
    let companion = state_handle.companion.lock().unwrap();
    companion.clone()
}
