export type ReminderType = "water" | "break" | "workout" | "idleBark";

export interface ReminderAudio {
  base64Wav: string;
  durationMs: number;
}

export interface ActiveReminder {
  type: ReminderType;
  message: string;
  triggeredAt: number;
  audio?: ReminderAudio;
}

export type GlowIntensity = "low" | "high";

export type ScreenCorner =
  "top-left" | "top-right" | "bottom-left" | "bottom-right";

export interface UpdateInfo {
  version: string;
  notes: string;
}

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
  updateAvailable: UpdateInfo | null;
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
  voice: {
    enabled: boolean;
    language: "en" | "ja";
    volume: number;
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
  // - synthesizeFlavorLine: voices an eye-click flavor line on demand —
  //   these are picked client-side and never touch CompanionState, so they
  //   can't ride along on the reminder pipeline's audio. See
  //   docs/VOICE_SPEC.md "Flavor-line voicing".
  // - getFlavorLines: fetches the canonical English flavor-line text once
  //   at startup (src-tauri/src/dialogues.rs is the source of truth) so
  //   the Renderer can keep picking/displaying locally afterward, same
  //   instant click response as before.
  // - setFlavorPopupVisible: tells Shell's hover watcher a flavor popup is
  //   on screen, so it won't re-engage click-through (and inadvertently
  //   make the popup vanish) until it's dismissed. Reminders don't need
  //   this — Shell already tracks Mode::Reminder itself.
  closeSettings: "close_settings",
  listMonitors: "list_monitors",
  getState: "get_state",
  reportEyeBounds: "report_eye_bounds",
  synthesizeFlavorLine: "synthesize_flavor_line",
  getFlavorLines: "get_flavor_lines",
  setFlavorPopupVisible: "set_flavor_popup_visible",
  // - installUpdate: downloads, installs, and relaunches into the update
  //   found by the launch-time check in src-tauri/src/updater.rs. Only
  //   meaningful while state.updateAvailable is set — see Renderer's
  //   click-routing logic.
  installUpdate: "install_update",
} as const;
