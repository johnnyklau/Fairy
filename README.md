# Fairy

A small always-on-top desktop companion: a transparent, borderless eye icon
that sits in a screen corner and shows wellness reminders (water, breaks,
workouts).

## Stack

Tauri v2 (Rust backend + plain TS/HTML/CSS frontend, no framework).

## Architecture

Six modules, matching `ARCHITECTURE.md` / `STATE_SCHEMA.md` / `VISUAL_SPEC.md`:

| Module | Where | Responsibility |
|---|---|---|
| Shell | `src-tauri/src/shell.rs` | Window, tray, click-through/hover detection, screen positioning |
| Renderer | `src/renderer/` | Eye SVG + popup bubble, purely reactive to state |
| Behavior | `src-tauri/src/behavior.rs` | Reminder scheduler — the only thing that decides *when* a reminder fires |
| State/Storage | `src-tauri/src/state.rs`, `src/state/` | Shared `CompanionState`, `Settings` persistence, IPC contract |
| Settings UI | `src/settings-ui/` | Reminder toggles/intervals, screen corner + monitor picker |
| Packaging | `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` | Bundling into installers |

`CompanionState` is runtime-only (rebuilt fresh on launch). `Settings` persists
across restarts via Tauri's store plugin, written to
`%APPDATA%\com.fairy.app\settings.json`.

## Development

```bash
npm install
npm run tauri dev
```

The Rust build directory is redirected to `%LOCALAPPDATA%\fairy-cargo-target`
(see `src-tauri/.cargo/config.toml`) rather than the default `src-tauri/target`,
since this project lives under OneDrive and its file-sync locking breaks
Cargo's rapid build-directory writes.

## Building installers

```bash
npm run tauri build
```

Produces an MSI and an NSIS installer under
`%LOCALAPPDATA%\fairy-cargo-target\release\bundle\`.

## Notes

- The eye is fixed-size and click-through everywhere except its own bounds —
  Shell polls the OS-level cursor position to detect hovering, since a
  click-through window can't receive the `mouseenter` event that would
  otherwise ask to un-block itself. The Renderer reports the eye's real
  rendered position (`getBoundingClientRect()`) to Shell on every
  layout-affecting render, so this never depends on hardcoded CSS values.
- Settings can be adjusted only from the tray icon ("Open Settings") — the
  eye itself is not a settings entry point, only a click-for-flavor-text toy.
