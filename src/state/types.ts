export type ReminderType = "water" | "break" | "workout" | "idleBark";

export interface ActiveReminder {
  type: ReminderType;
  message: string;
  triggeredAt: number;
}

export type GlowIntensity = "low" | "high";

export type ScreenCorner =
  "top-left" | "top-right" | "bottom-left" | "bottom-right";

export interface CompanionState {
  mode: "idle" | "reminder" | "settingsOpen";
  activeReminder: ActiveReminder | null;
  eye: {
    glowIntensity: GlowIntensity;
  };
  window: {
    clickThrough: boolean;
    corner: ScreenCorner;
  };
}

export interface Settings {
  water: {
    enabled: boolean;
    intervalMinutes: number;
  };
  breakReminder: {
    enabled: boolean;
    intervalMinutes: number;
  };
  workout: {
    enabled: boolean;
    timeOfDay: string;
  };
  idleBark: {
    enabled: boolean;
  };
  position: {
    corner: ScreenCorner;
    monitorIndex: number;
  };
  autostart: {
    enabled: boolean;
  };
}

export interface MonitorInfo {
  index: number;
  name: string | null;
  width: number;
  height: number;
}

export const IpcChannel = {
  getSettings: "get_settings",
  updateSettings: "update_settings",
  stateChanged: "state_changed",
  // Not in STATE_SCHEMA.md's IPC table:
  // - closeSettings: lets the Settings UI window close itself (opening
  //   happens tray-side, in Rust, without invoke).
  // - listMonitors: populates the position picker.
  // - getState: fetches a CompanionState snapshot on load, since an emit
  //   sent before the frontend's listener attaches would otherwise be lost.
  // - reportEyeBounds: gives Shell the eye's real rendered position, so
  //   hover/click-through hit-testing never hardcodes CSS layout values.
  closeSettings: "close_settings",
  listMonitors: "list_monitors",
  getState: "get_state",
  reportEyeBounds: "report_eye_bounds",
} as const;
