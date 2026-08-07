> Original v1 planning doc, used to scaffold the module layout. A few
> implementation details have since diverged (see README.md's "Notes"
> section) — most notably `set_click_through` ended up Shell-internal
> rather than Renderer-invoked, since a click-through window can't receive
> the `mouseenter` event that would ask to un-block itself.

# Architecture — Companion App (Fairy)

## Stack

Tauri v2 (Rust shell + JS/TS frontend). No custom Rust expected for v1 — built-in Tauri APIs cover window, tray, notifications, storage, and timers.

## Modules

### 1. Shell

Owns the native window and app lifecycle.

- Transparent, frameless, always-on-top window
- Click-through toggling (ignore mouse events when idle, capture when hovering the eye)
- Tray icon (open settings, quit)
- App startup/shutdown, single-instance lock

### 2. Renderer

Draws the eye and popups. Pure presentation — no timer/business logic.

- Eye component: concentric-ring SVG, blue, glow-intensity driven by state
- Idle breathing animation (CSS/JS loop)
- Popup component: dark bubble + eye badge + text, per VISUAL_SPEC.md
- Reads current state from State/Storage, renders accordingly — does not decide _when_ to show anything

### 3. Behavior

Owns _when_ things happen. No DOM/rendering code.

- Timer/scheduler for water, break, workout reminders
- Determines when a reminder should fire based on settings (intervals, workout time)
- Emits state changes (e.g. "reminder:water" event) for Renderer to react to
- Handles auto-dismiss timing for popups
- Idle-bark scheduler (low-frequency, optional feature)

### 4. State/Storage

Shared source of truth + persistence.

- In-memory state object (see STATE_SCHEMA.md)
- Tauri `store` plugin for persisting settings across restarts
- Exposes read/write via Tauri commands (`invoke`) and events (`emit`/`listen`)

### 5. Settings UI

Configurable panel, opened via tray or clicking the eye.

- Toggle each reminder type on/off
- Set water/break intervals
- Set workout time
- Writes to State/Storage on change

### 6. Build/Packaging

- Tauri bundler config (icons, app metadata)
- Cross-platform build targets (Windows/macOS/Linux as needed)

## Data flow

```
Settings UI ──writes──▶ State/Storage ◀──reads/writes── Behavior (scheduler)
                              │
                              ▼ (emit event on change)
                          Renderer (eye + popup)
                              ▲
                        Shell (window/click-through)
```

- Behavior is the only module that decides _when_ a reminder fires.
- State/Storage is the only module that persists data or holds shared truth.
- Renderer is purely reactive — given a state, it draws it. No timers of its own.
- Shell only manages the OS window container — never touches reminder logic.

## Build order

1. Shell + State/Storage (foundation — window exists, state contract defined)
2. Behavior + Renderer in parallel (once state schema is locked, these don't depend on each other)
3. Settings UI (depends on State/Storage contract only)
4. Packaging (last)

## Agent assignment

Each numbered module above = one agent's scope. Hand each agent this file + STATE_SCHEMA.md + VISUAL_SPEC.md as shared context so their outputs interoperate without needing to read each other's code.
