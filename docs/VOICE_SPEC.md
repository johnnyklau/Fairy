# Voice Spec — Supertonic TTS (Companion App "Fairy")

> Feature doc for voiced reminders. Originally scoped around Kokoro TTS;
> spike-testing during implementation found real problems with Kokoro's
> available model bundles (see "Investigation history" at the bottom) and
> led to switching engines entirely, to **Supertonic 3**. This version
> reflects that final decision — read top-to-bottom as the current plan, not
> a chronological record. Update this file again if reality diverges further.

## Goal

Voice Fairy's scheduled reminders (water/break/workout) using
[Supertonic 3](https://github.com/supertone-inc/supertonic) (MIT license),
with a Settings toggle to switch the spoken language between English and
Japanese. Both languages ship together — Supertonic is a single
multilingual model, so there's no "English first, Japanese later"
sequencing benefit like there would have been with Kokoro's per-language
model bundles.

## Scope decisions (locked in)

- **Voiced content: everything** — the three scheduled reminders (water,
  break, workout), idle-bark lines, and the eye-click flavor lines
  (`renderer/index.ts`'s `FLAVOR_LINES`). Revised from the original
  reminders-only scope once voice quality was proven out via spiking.
- **Displayed text stays English; only the audio switches language.**
  Fairy has no UI localization anywhere else (buttons, Settings labels,
  etc. are English-only), so `voice.language = ja` does not translate what
  the popup shows — it only changes which language is _spoken_. This keeps
  scope to "voice the existing dialogue," not "localize the app." Every
  line therefore needs two variants: the existing English display string
  (unchanged) and a Japanese string used only as TTS input, looked up by
  the same identity (reminder kind, or array index for idle-bark/flavor).
- **Synthesis model: live, not pre-baked clips.** The user's stated direction
  is eventually voicing LLM-generated text (see `docs/NEXT_STEPS.md`, Idea
  1 — the AI planning agent), so the synthesis path must accept arbitrary
  text, not just a fixed lookup table. Today's three reminder strings are
  still fixed and repeat on every interval, so a **content-hash cache** sits
  in front of the live engine — first occurrence of a given (text, language)
  pair synthesizes for real; every repeat is a cache hit. This gets today's
  reminders effectively "pre-baked" performance-wise without giving up the
  live path future features need.
- **Japanese address term: マスター (masutā)** — matches the English "master"
  framing and Fairy's stated visual inspiration (Zenless Zone Zero).
- **Voice: `sid: 0`, both languages.** Unlike the original Kokoro plan
  (separate named voices per language, e.g. `af_heart` / `jf_alpha`),
  Supertonic's `sid: 0` was confirmed by ear for _both_ English and Japanese
  — same voice model speaking both languages, so Fairy's voice actually
  stays consistent across the language toggle, which the original two-model
  plan couldn't offer. See [Voice selection](#voice-selection).
- **Translation: hand-written, not machine-translated at runtime.** Only
  three strings need Japanese versions today. A runtime MT API/library would
  add a dependency (and possibly a cost) to translate a fixed, tiny string
  set — not worth it. See [Translations](#translations) for drafts.

## Out of scope for this pass

- A per-line voice picker in Settings UI (a single volume slider was added
  after this pass started — see "State/Settings schema changes" below).
- Locale-based auto-language-detection — language is a manual Settings
  toggle, per the original ask.
- Actually wiring this engine up to LLM-generated text — that's Idea 1's
  Agent module, not yet designed. This spec just needs to not make that
  integration harder later (hence live synthesis over static clips).

## Module: Voice

New module, sibling to Shell/Renderer/Behavior/State per `ARCHITECTURE.md`'s
pattern. Lives at `src-tauri/src/voice.rs`.

Owns:

- Loading the Supertonic model (once, lazily or eagerly — see
  [Model provisioning](#model-provisioning))
- `synthesize(text, language) -> AudioClip` — the live inference call
- The content-hash synthesis cache (in-memory + on-disk)
- Downloading/verifying the model on first enable

Does **not** own: deciding _when_ a reminder fires (still Behavior),
deciding _what the message says_ (Dialogues — see below), or playback
(still Renderer). This keeps the existing ownership boundary from
`STATE_SCHEMA.md` intact — Voice is a new capability, not a new decision-maker.

## Module: Dialogues

New module, `src-tauri/src/dialogues.rs`. Owns the actual dialogue
content: every reminder, idle-bark, and eye-click flavor line, in both
English (display) and Japanese (voice) form, plus **random variation
selection** — each message type has multiple phrasings, and one is picked
uniformly at random each time.

```rust
pub struct Line {
    pub en: &'static str,
    pub ja: &'static str,
}

impl Line {
    pub fn for_language(&self, language: VoiceLanguage) -> &'static str { /* .en or .ja */ }
}

pub fn pick(lines: &'static [Line]) -> &'static Line { /* rand::thread_rng().gen_range(..) */ }

pub const WATER_LINES: &[Line] = &[ /* 3 variations */ ];
pub const BREAK_LINES: &[Line] = &[ /* 3 variations */ ];
pub const WORKOUT_LINES: &[Line] = &[ /* 3 variations */ ];
pub const IDLE_BARK_LINES: &[Line] = &[ /* 4 variations */ ];
pub const FLAVOR_LINES: &[Line] = &[ /* 5 variations */ ];
```

**Why pair `en`/`ja` in one struct instead of two index-parallel arrays**:
that was the original design (a `FLAVOR_LINES` array in `renderer/index.ts`
plus a same-order `FLAVOR_LINES_JA` array in `voice.rs`, kept in sync only
by convention/comments). Once variations were added, "keep two arrays the
same length, same order, forever" became a real correctness risk, not just
a style nit — a single `Line { en, ja }` picked once makes desync
structurally impossible instead of relying on discipline.

**Why flavor lines' English moved here from the Renderer**: previously the
Renderer owned `FLAVOR_LINES` outright (its own hardcoded array, zero
backend involvement, so a click was instant). That's still true — the
Renderer still picks and displays locally on every click — but the
_content_ now lives here, fetched once at startup via a new
`get_flavor_lines` command (see [Flavor-line voicing](#flavor-line-voicing)).
This is what makes "everything Fairy says lives in one file" actually true,
rather than true for reminders/idle-bark only.

Reminders and idle-bark don't need an equivalent fetch — Behavior already
runs in the same Rust binary as Dialogues and calls `dialogues::pick(...)`
directly.

### Updated data flow

```
Settings UI ──writes──▶ State/Storage ◀──reads/writes── Behavior (scheduler)
                              │                              │
                              │                        calls Voice::synthesize
                              │                       (only if voice.enabled)
                              ▼ (emit event on change)        │
                          Renderer (eye + popup + audio) ◀────┘ (audio bytes
                              ▲                                 attached to
                        Shell (window/click-through)            activeReminder)
```

Behavior still fires the emit; it just now optionally attaches synthesized
audio to the payload before doing so.

## State/Settings schema changes

### `Settings` (Rust: `src-tauri/src/state.rs`)

```rust
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VoiceLanguage {
    En,
    Ja,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSettings {
    pub enabled: bool,
    pub language: VoiceLanguage,
    #[serde(default = "default_voice_volume")]
    pub volume: f32,
}

fn default_voice_volume() -> f32 {
    0.5
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self { enabled: false, language: VoiceLanguage::En, volume: default_voice_volume() }
    }
}
```

Add `#[serde(default)] pub voice: VoiceSettings` to `Settings`, matching how
`position`/`autostart` were added — old `settings.json` files missing the
key must still deserialize (write a migration test mirroring the existing
`autostart` migration test in `settings.rs`).

**Default is `enabled: false`.** `VISUAL_SPEC.md` explicitly specified "no
sound in v1" — adding audio should be opt-in, not a surprise for existing
users on their next update.

**`volume` defaults to `0.5`** (half of unattenuated output) and needs its
own field-level `#[serde(default = "...")]`, not just the struct-level one
on `Settings.voice` — `voice` shipped before `volume` existed, so real
`settings.json` files already exist with `voice: { enabled, language }`
and no `volume` key. Without the field-level default, deserializing that
object would fail outright and fall back to resetting the _entire_
`Settings` struct to defaults (water/break/workout/etc. included), not
just `voice`. `sanitize()` in `settings.rs` clamps it to `0.0..=1.0` on
every load/save, same defense-in-depth pattern as the interval clamps.

**Applied server-side, not baked into the synthesis cache.** `voice.rs`'s
content-hash cache always stores the canonical (unscaled) processed clip;
`synthesize()` takes `volume` as a parameter and re-encodes a scaled copy
on every call (`apply_volume()`, a cheap PCM decode-scale-reencode, not a
DSP re-run). This means moving the volume slider never invalidates the
cache or forces re-synthesis — only the final serve step changes. The
Renderer needs no changes for this: the WAV bytes it receives already
have the configured volume applied.

### `ActiveReminder` (Rust `state.rs` + TS `state/types.ts`)

```rust
#[derive(Clone, Serialize, Deserialize)]
pub struct ActiveReminder {
    #[serde(rename = "type")]
    pub kind: ReminderType,
    pub message: String,
    pub triggered_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<ReminderAudio>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderAudio {
    pub base64_wav: String,
    pub duration_ms: u32,
}
```

```ts
export interface ActiveReminder {
  type: ReminderType;
  message: string;
  triggeredAt: number;
  audio?: { base64Wav: string; durationMs: number };
}

// Settings, extended:
export interface Settings {
  // ...existing fields
  voice: {
    enabled: boolean;
    language: "en" | "ja";
  };
}
```

Audio ships as base64 WAV inline in the existing `state_changed` emit — no
new IPC channel needed. A few hundred KB per reminder over local IPC is
irrelevant; avoids the complexity of a temp-file + `asset://` protocol setup
for what's fundamentally a short clip.

No new Tauri command is required for playback. Settings' existing
`get_settings`/`update_settings` commands already carry the new `voice`
field through unchanged.

## Behavior changes (`behavior.rs`)

`fire()` currently hardcodes `REMINDER_DISPLAY = 5s` for every reminder, and
is a plain synchronous function called directly from `tick()`. Change:

1. **`fire()` and `tick()` both become `async fn`.** The scheduler loop
   already runs inside an async task (`start_scheduler`'s
   `tauri::async_runtime::spawn`), so this adds `.await` points to an
   existing async context rather than introducing a new execution model.
   Because the scheduler loop calls `tick(&app, &timers).await` and only
   then loops back to `sleep(TICK).await`, no second `tick()` can run
   concurrently while a reminder is mid-synthesis — the loop is naturally
   serialized, so this doesn't reopen a double-fire race.
2. **If `settings.voice.enabled`, `await voice::synthesize(line.for_language(settings.voice.language),
settings.voice.language)` before calling `set_reminder(...)`**, where
   `line` is a [`dialogues::Line`](#module-dialogues) picked once via
   `dialogues::pick(...)` and threaded through as a single value — `line.en`
   is what's displayed, `line.for_language(...)` is what's spoken, per the
   "display stays English" scope decision (see [Translations](#translations)).
   Picking one `Line` up front, instead of a separate index + two lookups,
   is what makes "display and voice always match" a compiler-checked fact
   rather than a "reuse the same index" discipline. The user confirmed the
   wait-for-audio tradeoff explicitly isn't a concern ("I'd be more worried
   if it was 10 minutes late") — the popup waits until audio is ready rather
   than showing early and patching audio in later, for simpler code and a
   synced audio+visual reveal. `synthesize()` must enforce its own internal
   timeout (suggest `SYNTHESIS_TIMEOUT = 3s`, comfortably above the ~0.7s
   observed in spike testing) so a stuck or slow model call can't hang the
   popup indefinitely — this is the "timeout" failure case referenced in
   step 4.
3. On success: attach the returned `ReminderAudio` to `set_reminder(...)`,
   and set the dismiss delay to `max(REMINDER_DISPLAY, clip.duration + AUDIO_TAIL_BUFFER)`
   (suggest `AUDIO_TAIL_BUFFER = 500ms`) so the popup never disappears
   mid-sentence. Only one dismiss timer is ever scheduled per reminder —
   audio (if any) is already known by the time `set_reminder` is called, so
   there's no race between an early fixed-duration timer and a later
   audio-duration one.
4. On failure (model not downloaded yet, synthesis error, or the timeout
   from step 2): log and fall back to calling `set_reminder(...)`
   immediately with no audio and the existing fixed `REMINDER_DISPLAY`,
   same resilience pattern already used throughout `settings.rs`/
   `shell.rs` (`let Ok(...) = ... else { return }`).

Since the popup now waits on synthesis when voice is enabled, eager
background model load at startup (see
[Synthesis pipeline & caching](#synthesis-pipeline--caching)) matters more
than a nice-to-have: a cold model load on the first reminder of a session
would otherwise stack on top of per-call inference time, making that first
delay noticeably longer than subsequent ones.

This also means Mode stays `Reminder` (blocking other reminders from firing,
per the existing "one thing at a time" rule) for the full audio duration,
not just 5s — consistent with `VISUAL_SPEC.md`'s "no multiple popups
stacked" rule.

## Renderer changes (`renderer/index.ts`)

In `renderState()`, when `state.activeReminder?.audio` is present: decode
the base64 WAV into a `Blob`, create an object URL, play via
`new Audio(url)`, and revoke the URL on `ended` or when the reminder clears.
Renderer stays purely reactive for reminders — it plays whatever audio is
attached to state, the same way it already renders whatever text is
attached. It does not decide whether to synthesize, retry, or fall back —
that's Behavior/Voice.

Eye-click flavor lines are the one exception to "Renderer never talks to
Voice directly" — see below, since they're picked client-side and never
touch `CompanionState`.

## Flavor-line voicing

Flavor lines are chosen locally on click and never go through
`Behavior`/`CompanionState` — voicing them can't reuse the reminder
pipeline. Their content lives in `dialogues::FLAVOR_LINES` (see
[Module: Dialogues](#module-dialogues)), fetched by the Renderer once at
startup:

```rust
#[tauri::command]
pub fn get_flavor_lines() -> Vec<String> // dialogues::FLAVOR_LINES[i].en, in order

#[tauri::command]
pub async fn synthesize_flavor_line(app: AppHandle, index: u32) -> Option<ReminderAudio>
```

`synthesize_flavor_line` looks up `dialogues::FLAVOR_LINES[index]` directly
(no separate English parameter needed — Dialogues is canonical, so the
Renderer doesn't need to send back text it only just fetched from the same
place), voices it via `line.for_language(settings.voice.language)`, and
returns `None` if the index is out of range, voice is disabled, the model
isn't ready, or synthesis times out/fails — same resilience contract as
reminders.

Renderer's `initRenderer()` fetches the list once:

```ts
let flavorLines: string[] = [];
void getFlavorLines().then((lines) => {
  flavorLines = lines;
});
```

and the click handler picks an index into that cached copy — same instant
local pick as before, just backed by fetched data instead of a hardcoded
array — then, mirroring the "popup waits for audio" choice made for
reminders (so the click doesn't show English text while Japanese audio
arrives half a second later out of sync), awaits `synthesizeFlavorLine(index)`
before showing the popup:

```ts
async function handleFlavorClick(): Promise<void> {
  if (flavorLines.length === 0) return; // not fetched yet
  const index = Math.floor(Math.random() * flavorLines.length);
  const line = flavorLines[index];
  const audio = await synthesizeFlavorLine(index).catch(() => null);
  setPopupText(popupEl, line);
  showPopup(popupEl);
  if (audio) playBase64Wav(audio.base64Wav);
  clearTimeout(flavorTimeout);
  const displayMs = Math.max(
    FLAVOR_DISPLAY_MS,
    (audio?.durationMs ?? 0) + AUDIO_TAIL_BUFFER_MS,
  );
  flavorTimeout = setTimeout(() => hidePopup(popupEl), displayMs);
}
```

If voice is disabled, `synthesizeFlavorLine` resolves to `null` without a
meaningful delay — the command returns immediately when
`!settings.voice.enabled`, so the `await` costs one cheap IPC call, not a
synthesis wait. The one-time `get_flavor_lines` fetch at startup is the
only added latency versus the original hardcoded-array design, and it
happens well before the user's first click in practice.

## Synthesis pipeline & caching

- **Cache key**: hash of `(text, language)`. Voice is always `sid: 0`
  regardless of language, so it doesn't need to be part of the key today —
  though including it wouldn't hurt if a future per-language voice override
  is ever added. In-memory `HashMap` for the process lifetime, backed by
  on-disk WAV files under the app data dir (e.g.
  `%APPDATA%\com.fairy.app\voice_cache\<hash>.wav`) so a restart doesn't
  re-synthesize the same three reminder strings again.
- **Model load**: lazy on first synthesis call, _or_ kicked off as a
  background async task at startup when `voice.enabled` (same
  fire-and-forget spawn pattern already used for `start_hover_watcher` /
  `start_scheduler` in `lib.rs`'s `.setup()`), so the first real reminder
  isn't delayed by a multi-second cold model load. Recommend eager
  background load — the cost is paid once, off the critical path.
- **Output format**: raw PCM from Supertonic (**44100 Hz mono** — not
  Kokoro's 24000 Hz, a leftover assumption from the original plan) wrapped
  in a minimal WAV header. Read the sample rate from `tts.sample_rate()` at
  runtime rather than hardcoding it, in case that ever changes with a model
  update. No external audio encoding dependency needed; browsers play
  `audio/wav` natively.

## Model provisioning

Supertonic 3's ONNX weights (int8-quantized: `duration_predictor.int8.onnx`,
`text_encoder.int8.onnx`, `vector_estimator.int8.onnx`, `vocoder.int8.onnx`,
plus `tts.json`, `unicode_indexer.bin`, `voice.bin`) total **~145MB** —
confirmed by direct download during spiking, not an estimate. One download
covers both languages (unlike the original Kokoro plan, which would have
needed a second, separate model for Japanese). Too large to bundle into the
installer for an app whose whole current footprint is a few MB — **download
on first enable**, not bundled:

1. User flips the Voice toggle on in Settings for the first time.
2. If the model isn't present in the app data dir, download it (from the
   [sherpa-onnx GitHub release](https://github.com/k2-fsa/sherpa-onnx/releases/tag/tts-models),
   asset `sherpa-onnx-supertonic-3-tts-int8-2026-05-11.tar.bz2` at spec-writing
   time — re-check for a newer dated release at implementation time) with a
   progress indicator — needs a new event channel (e.g.
   `voice_model_download_progress`, payload `{ downloadedBytes, totalBytes }`)
   for the Settings UI to show a progress bar.
3. Extract the `.tar.bz2` archive into the app data dir.
4. Verify a checksum against the downloaded file before treating it as
   ready.
5. On download failure (no internet, interrupted): show an error in
   Settings, leave `voice.enabled` reverted to `false` (or a distinct
   "enabled but not ready" state — implementer's call), and reminders
   continue to fire silently until the user retries.

This keeps the base app small and only costs bandwidth for users who opt in.

## Voice selection

Supertonic exposes 10 speaker IDs (`sid: 0`–`9`) via `voice.bin`; unlike
Kokoro, none are documented with names/genders in Supertonic's own docs as
of spec-writing time.

**Locked in: `sid: 0`, both languages.** Confirmed by ear against sids 0–5
on the actual English and Japanese reminder lines (separately) — `sid: 0`
picked for both. This is a single constant in `voice.rs`, used regardless of
`settings.voice.language`; only the `lang` extra param passed to
`generate_with_config` changes between English and Japanese, not the voice
ID.

## Voice effects chain (locked in, not yet implemented)

Exploratory add-on, prompted by a reference site
([soundtools.io/voice-changer](https://soundtools.io/voice-changer/)) the
user wanted to approximate — **not a byte-for-byte clone of that site's
implementation** (no access to its source), but a hand-rolled DSP chain
built from standard formulas and confirmed by ear against both the English
and Japanese reminder lines using real Supertonic output. Spiked in a
standalone scratch Cargo project outside this repo, not yet ported into
`voice.rs`.

**Confirmed parameters**, applied in this order to the raw Supertonic PCM:

1. **Pitch shift**: +3 semitones, phase vocoder (`pitch_shift` crate, MIT).
   Naive resampling was rejected — it changes speed along with pitch (the
   "chipmunk effect"); a phase vocoder shifts pitch independent of duration.
2. **Distortion**: `tanh` saturation, drive amount `0.2`
   (`drive = 1.0 + amount * 15.0`, then `sample.tanh()`).
3. **Bitcrush**: 11-bit depth, 24% wet mix.
4. **Vibrato**: depth `0.18`, rate `1.0 Hz`, 10% wet mix (variable-delay LFO
   modulation, linear-interpolated read).
5. **Reverb**: Schroeder-style (4 parallel comb filters + 2 series allpass
   filters, RT60-derived feedback), 0.2s decay, 19% wet mix.
6. **EQ**: three RBJ Audio EQ Cookbook biquads in series — low shelf 200Hz
   `-3dB`, mid peak 1000Hz `+4dB`, high shelf 4000Hz `-10dB`.

All six stages are hand-implemented (no DSP framework dependency) to avoid
taking on an unconfirmed external API (`fundsp`'s `Wave` construction
couldn't be pinned down from docs during spiking) and to keep the chain's
behavior fully auditable.

**Known issue — clipping headroom**: at some tested parameter combinations
(e.g. pitch +3.5 semitones with distortion 0.2) the chain's output peaked
above `1.0` (`max_abs=1.045`), which the current spike's naive
clamp-on-write silently hard-clips. The locked-in parameters above (pitch
+3, distortion 0.2) stayed under the ceiling (`max_abs=0.901`), but if this
chain is ported into `voice.rs`, add a peak-normalize or soft-limiter pass
after stage 6 so future parameter tweaks (or different reminder text/
language producing louder synthesis) can't silently distort the shipped
audio.

**Decided: always-on, baked into `voice::synthesize`.** No separate Settings
toggle — every synthesized reminder gets the processed voice whenever
`voice.enabled` is true. This is in scope for the current implementation
pass (tasks #12-21), not a deferred follow-up: `voice.rs` should run the raw
Supertonic PCM through all six stages above (plus the limiter/normalize
pass noted under "Known issue") before wrapping it as a `ReminderAudio`, so
the cached WAV that ships to the Renderer is already the final processed
clip.

## Translations

All dialogue content — English display text, Japanese voice text, and now
multiple random-selected variations per message — lives in
`src-tauri/src/dialogues.rs` (see [Module: Dialogues](#module-dialogues)),
not in this doc. Draft Japanese lines throughout use マスター per the
locked decision. **These are drafts for your (or a native speaker's) review
before shipping** — translation nuance isn't something to take on faith
from a spec doc.

This section used to hold the actual line tables directly — that
duplicated `dialogues.rs`, and once variations were added would have meant
keeping a growing set of phrasings in sync in two places. Read
`dialogues.rs` directly for current content; at time of writing:

| Category          | Variations | Selection                                                                                                                        |
| ----------------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `WATER_LINES`     | 3          | `dialogues::pick(...)`, called from Behavior                                                                                     |
| `BREAK_LINES`     | 3          | same                                                                                                                             |
| `WORKOUT_LINES`   | 3          | same                                                                                                                             |
| `IDLE_BARK_LINES` | 4          | same                                                                                                                             |
| `FLAVOR_LINES`    | 5          | picked client-side in the Renderer, from a copy fetched via `get_flavor_lines` — see [Flavor-line voicing](#flavor-line-voicing) |

Adding a variation to any category is a one-line addition to its `Line`
array in `dialogues.rs` — no other file needs to change, which is the
point of consolidating this content into one module.

## Settings UI changes (`settings-ui/index.ts`)

New fieldset, matching the existing pattern (Water/Break/Workout/Idle
bark/Position fieldsets):

```html
<fieldset>
  <legend>Voice</legend>
  <label><input type="checkbox" name="voiceEnabled" /> Enabled</label>
  <label
    >Language
    <select name="voiceLanguage">
      <option value="en">English</option>
      <option value="ja">Japanese</option>
    </select>
  </label>
  <label
    >Volume
    <input type="range" name="voiceVolume" min="0" max="1" step="0.01" />
  </label>
  <!-- if a model download is in progress, show progress here -->
</fieldset>
```

Wire up the same `applySettings`/`handleChange` round-trip already used for
every other field — all three voice fields (`enabled`/`language`/`volume`)
are sent together on any single change, same convention as every other
multi-field group (water, workout, etc.). If a model download is triggered
by enabling voice, listen for the new `voice_model_download_progress`
event and show a simple progress bar or percentage text inline in this
fieldset. Since one model covers both languages, switching the language
dropdown after the model is already downloaded never triggers a second
download. The volume slider takes effect on the _next_ synthesized clip —
see "Applied server-side, not baked into the synthesis cache" above — not
retroactively on whatever's already playing.

## Testing plan

Mirror the project's existing rigor (27 Rust tests, 20 frontend tests as of
`docs/`'s last update — see `README.md`):

- **Rust**: unit-test the cache key derivation, the dismiss-duration math
  (`max(REMINDER_DISPLAY, clip_duration + buffer)`), the WAV-header wrapping
  given known PCM input (using the real 44100 Hz rate), and the
  `VoiceSettings` default + migration (old `settings.json` without a
  `voice` key). **Do not** unit-test actual model inference — that needs
  the downloaded model and is an integration concern, not a fast unit test;
  keep the synthesis call itself behind a trait/interface so it can be
  mocked in tests, same spirit as how `elapsed_at_least`/`should_fire_workout`
  in `behavior.rs` are pure functions isolated from `AppHandle`.
- **Frontend**: vitest coverage for the new Settings fieldset's
  populate/patch round-trip (same shape as the existing tests in
  `settings-ui/index.test.ts`), and for `renderState()`'s audio-attach
  branch (asserting an `<audio>` element gets created/played when
  `activeReminder.audio` is present, and doesn't when it's absent).

## Build/installer impact

- No new bundled binary size from the model itself (downloaded, not
  bundled). `sherpa-onnx`'s native ONNX Runtime linking (statically linked
  by default, confirmed via spike build) does add to the compiled binary
  size — not yet measured precisely against the current installer, worth
  checking once integrated into `src-tauri` for real.
- `cargo clippy -D warnings` and `cargo fmt` must stay clean, per existing
  CI (`.github/workflows/ci.yml`).

## Open risks / implementation notes for Claude Code

1. **Supertonic voice sid → name/gender mapping isn't published** the way
   Kokoro's was. `sid: 0` is confirmed to sound right for both languages by
   direct listening, but there's no official docs page to cite for "this is
   voice X, gender Y" the way there was for Kokoro's sid tables. Not
   blocking, just means the [Voice selection](#voice-selection) section
   above is the source of truth, not an external voice manifest.
2. Confirm Supertonic 3's actual license terms for redistribution/bundling
   are still MIT at implementation time (they were at spec-writing time) —
   this affects whether shipping the model at all is clean. Separately, the
   `sherpa-onnx` crate itself is Apache-2.0 — two different licenses cover
   two different artifacts (the inference bindings vs. the model weights),
   don't conflate them when documenting this in the app.
3. Decide the exact "enabled but model not ready yet" UI state — this spec
   leaves the precise Settings-UI treatment (spinner? disabled toggle?
   error text?) to the implementer's judgment.
4. Only one English and one Japanese sentence were spike-tested each
   (single reminder line per language, not all three, and no stress-testing
   of unusual words). The `ɚ`-style silent-phoneme-drop failure mode found
   during Kokoro investigation (see below) is a real category of bug, not
   necessarily specific to Kokoro — worth listening carefully to all three
   real reminder lines in both languages once implemented, not just
   trusting that "the spike sentence worked" generalizes.

## Investigation history — why Kokoro was rejected

Kept for context and because the underlying lesson (silent phoneme-dropping)
applies beyond just Kokoro. Not the current plan — see the rest of this
document for that.

- **Crate evaluated and confirmed good**:
  [`sherpa-onnx`](https://crates.io/crates/sherpa-onnx) (k2-fsa, Apache-2.0,
  official Rust bindings, 164k+ downloads) — this part of the decision
  _carried over_ to the Supertonic plan too; it wasn't Kokoro-specific.
  Chosen over the smaller Kokoro-specific wrapper crates (`kokoro-tts`,
  `kokoro-en`, etc.), which had far lower adoption and, in `kokoro-tts`'s
  case, ~1% doc coverage.
- **Kokoro model bug found**: `kokoro-multi-lang-v1_1`'s `tokens.txt` is
  **missing the IPA symbol U+025A (ɚ, the "-er" rhotic vowel)** — words
  like "water" and "master" silently lost their endings ("wat", "mast")
  because that phoneme gets dropped rather than erroring. Confirmed via
  direct inspection: the lexicon file has the _correct_ decomposed
  phonemes (`ə` + `ɹ`, both valid tokens) for both words, but something
  falls through to live espeak-ng phonemization instead of the lexicon
  anyway, which emits the combined `ɚ` symbol — not in this model's
  vocabulary, so silently skipped. `kokoro-en-v0_19` (the English-only
  Kokoro bundle)'s `tokens.txt` _does_ include `ɚ`, and switching to it
  fixed the English clip completely.
  - **The generalizable lesson**: don't assume any TTS model bundle's
    tokenizer is complete just because it loads without error — dropped
    phonemes can fail _silently_ (a log warning, not an exception), so a
    truncated-sounding word is the only symptom. If a future model swap
    (Supertonic or otherwise) produces clipped-sounding words, check the
    model's token/phoneme vocabulary for the symbols in question before
    assuming it's a code bug.
- **Why Kokoro was dropped entirely, not just the buggy bundle**: Kokoro
  has no official Japanese-capable bundle in sherpa-onnx's packaging at
  all (`kokoro-en-v0_19` and both multi-lang bundles are English/Chinese
  only) — an early search suggesting a `jf_alpha` Japanese voice existed
  didn't hold up against sherpa-onnx's own model-zoo docs. That would have
  meant a second, different, voice-inconsistent engine for Japanese
  regardless of which English Kokoro bundle was used. Supertonic 3 solves
  both problems at once: no missing-phoneme bug encountered, and one
  model/voice for both languages instead of two.
