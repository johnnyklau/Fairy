> Original v1 planning doc. The implementation added a few fields/channels
> beyond this spec (window position/corner, monitor picker, eye-bounds
> reporting, get_state) — see README.md and the `IpcChannel` comments in
> `src/state/types.ts` for what's actually shipped.

# State Schema — Companion App (Fairy)

## Shared state object

```ts
type CompanionState = {
  // current visual/behavior mode
  mode: "idle" | "reminder" | "settingsOpen";

  // active reminder, if mode === "reminder"
  activeReminder: {
    type: "water" | "break" | "workout" | "idleBark";
    message: string; // popup text
    triggeredAt: number; // epoch ms
  } | null;

  // eye visual state
  eye: {
    glowIntensity: "low" | "high"; // low = idle, high = active reminder
  };

  // window/interaction state (owned by Shell, read by Renderer)
  window: {
    clickThrough: boolean;
  };
};
```

## Settings object (persisted)

```ts
type Settings = {
  water: {
    enabled: boolean;
    intervalMinutes: number; // e.g. 60
  };
  breakReminder: {
    enabled: boolean;
    intervalMinutes: number; // e.g. 50
  };
  workout: {
    enabled: boolean;
    timeOfDay: string; // "HH:mm", 24hr, e.g. "18:00"
  };
  idleBark: {
    enabled: boolean;
  };
};
```

## Storage keys (Tauri store plugin)

- `settings` → `Settings` object above
- No persistence needed for `CompanionState` — it's runtime-only, rebuilt fresh on app start (defaults to `mode: "idle"`)

## IPC contract (Tauri invoke/emit)

| Channel             | Direction                   | Payload             | Purpose                                       |
| ------------------- | --------------------------- | ------------------- | --------------------------------------------- |
| `get_settings`      | Renderer → Backend (invoke) | none                | Fetch current settings on load                |
| `update_settings`   | Renderer → Backend (invoke) | `Partial<Settings>` | Save settings change                          |
| `state_changed`     | Backend → Renderer (emit)   | `CompanionState`    | Behavior pushes new state whenever it changes |
| `set_click_through` | Renderer → Backend (invoke) | `boolean`           | Toggle window click-through (Shell)           |

## Ownership

- **Behavior** is the only module allowed to write `mode`, `activeReminder`, `eye.glowIntensity`.
- **Shell** is the only module allowed to write `window.clickThrough`.
- **Settings UI** is the only module allowed to write `Settings`.
- **Renderer** never writes state — read-only consumer via `state_changed` events.

## Reminder message examples (used to populate `activeReminder.message`)

- Water: "Time to drink some water, master."
- Break: "Stand up and stretch for 5, master."
- Workout: "Workout time, master."
- Idle bark: rotating pool of flavor lines, non-actionable
