import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  type CompanionState,
  DEFAULT_COMPANION_STATE,
  IpcChannel,
  type MonitorInfo,
  type Settings,
} from "./types";

export * from "./types";

export function getSettings(): Promise<Settings> {
  return invoke<Settings>(IpcChannel.getSettings);
}

export function updateSettings(patch: Partial<Settings>): Promise<Settings> {
  return invoke<Settings>(IpcChannel.updateSettings, { settings: patch });
}

export function setClickThrough(clickThrough: boolean): Promise<void> {
  return invoke(IpcChannel.setClickThrough, { clickThrough });
}

export function closeSettings(): Promise<void> {
  return invoke(IpcChannel.closeSettings);
}

export function listMonitors(): Promise<MonitorInfo[]> {
  return invoke<MonitorInfo[]>(IpcChannel.listMonitors);
}

export function getState(): Promise<CompanionState> {
  return invoke<CompanionState>(IpcChannel.getState);
}

export function reportEyeBounds(
  left: number,
  top: number,
  width: number,
  height: number,
): Promise<void> {
  return invoke(IpcChannel.reportEyeBounds, {
    bounds: { left, top, width, height },
  });
}

function onStateChanged(
  callback: (state: CompanionState) => void,
): Promise<UnlistenFn> {
  return listen<CompanionState>(IpcChannel.stateChanged, (event) => {
    callback(event.payload);
  });
}

let currentState: CompanionState = DEFAULT_COMPANION_STATE;

export function getCurrentState(): CompanionState {
  return currentState;
}

export function initState(onChange: (state: CompanionState) => void): void {
  void getState().then((state) => {
    currentState = state;
    onChange(state);
  });
  void onStateChanged((state) => {
    currentState = state;
    onChange(state);
  });
}
