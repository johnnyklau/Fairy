# Fairy vNext — Brainstormed Ideas

This is a standalone context doc for two feature ideas discussed for **Fairy**, a Tauri v2 desktop companion app (see project's `ARCHITECTURE.md`, `STATE_SCHEMA.md`, and other project instructions for the base v1 app — an always-on-top transparent window with an animated eye that delivers wellness reminders: water, breaks, workouts).

These ideas are **not part of v1 scope**. They represent a "vNext" expansion, audited against a budget constraint of **up to $20/year in service costs** (revised up from an original $0 constraint).

---

## Status update — Voice has graduated to a real spec

Voice output (originally just the "Text-to-speech" bullet under Idea 1 below)
now has an actual implementation spec: see `docs/VOICE_SPEC.md`. Decisions
there intentionally supersede what's written in Idea 1 — this doc is a
brainstorm, not a spec to follow to the letter, and the more specific doc
wins:

- **Engine: Supertonic 3 (via the `sherpa-onnx` crate), not Piper or
  Kokoro.** Piper was the original brainstorm guess; Kokoro was the first
  real spec choice, but spike-testing found a silent phoneme-dropping bug
  in Kokoro's available model bundles and no working Japanese bundle at
  all (see `VOICE_SPEC.md`'s "Investigation history"). Supertonic 3 fixed
  both — no phoneme bug encountered, and one multilingual model instead of
  a separate bundle per language.
- **Japanese is no longer a stretch goal — it ships alongside English.**
  The original Python-sidecar fallback concern was specific to Kokoro's
  MeCab-based Japanese phonemization pipeline. Supertonic sidesteps that
  entirely: it's the _same_ model and Rust pipeline as English, with
  language selected by a runtime parameter, not a separate model/toolchain.
  Both English and Japanese were spike-tested end-to-end and confirmed
  sounding correct before this decision was made.

---

## Idea 1 — AI planning agent

**Concept**: A daily conversational agent. You describe your day out loud, and it plans water/break/workout reminders around it — fitting them into your day, adjusting for events, deciding when things don't fit.

**Voice I/O**:

- Speech-to-text: `whisper.cpp` — local, free, good quality even on small models. No reason to change this regardless of budget.
- Text-to-speech: `Piper` — local, free, neural voices. Also stays local regardless of budget (no cost benefit to a cloud TTS here).

**Reasoning / planning (the LLM)**:

- **Decision: use the Claude API**, not a local model. This is a **separate product from Claude Pro** — Pro is the claude.ai chat subscription; the API requires its own key and its own pay-as-you-go billing, even under the same Anthropic account. Claude Pro does **not** grant free API usage.
- Estimated cost: roughly **$1/year** for once-daily use on Claude Haiku (a single planning call is ~1-2k tokens total). Comfortably inside the $20/year budget, with room to spare.
- Local-model alternative (Ollama + Phi-3-mini or Llama 3.2 3B) was considered and rejected in favor of cloud quality, now that budget allows it. Local remains a fallback if API access is ever undesirable (e.g. offline use).

**Architecture**:

- LLM has no memory between calls — any "remembered" context (your habits, prior conversations) must be stored in Fairy's own state/settings and re-supplied in each prompt.
- LLM should return **structured output** (e.g. JSON: `{ reminders: [...], summary: "..." }`), not free text — Fairy's code parses this directly. The LLM does language understanding; app code does the actual scheduling and state writes.
- Not an autonomous "agent loop" — this is a single request/response once a day, not repeated LLM calls taking actions. Simpler and easier to debug.
- Proposed as a new module: **Agent**. Sits alongside Behavior. Calls the Claude API for parsing/planning, then hands computed reminder times to Behavior (which remains the only module writing `mode`, `activeReminder`, `eye.glowIntensity` per existing ownership rules).

**Multi-day reasoning (decided)**:

- Agent must be able to plan across more than a single day. Example use case: "I have a physio appointment next Thursday, schedule three workout sessions before then, spread appropriately."
- Requires a new **persisted Plan structure** (not just runtime `CompanionState`, which is explicitly single-day/runtime-only per `STATE_SCHEMA.md`) — stored in the Tauri `store`, owned by Agent, surviving app restarts.
- **Split of responsibility, explicitly decided**: the LLM's job is to extract structured intent from speech (e.g. "3 workout sessions, physio-related, deadline next Thursday"). The actual _placement_ — spacing sessions, avoiding calendar conflicts, respecting the deadline — is a deterministic constraint-fitting algorithm, not an LLM call. This keeps placement reliable and reviewable; the LLM is not trusted to silently get scheduling math right.
- Behavior still only fires _today's_ portion of whatever plan Agent has computed.

**Smart rescheduling / conflict awareness (decided)**:

- Beyond initial planning, Agent should also help adapt the plan when circumstances change. Two motivating examples:
  1. A same-day event appears unexpectedly (e.g. a volleyball session) — Fairy should consider pushing back or skipping a workout for recovery.
  2. The user reports feeling physically unready for a planned intensive session — Fairy should consider a lighter substitute or a reschedule/skip, and note that a lighter session doesn't count toward physio compliance.
- **This is explicitly scoped as an LLM-judgment task, not a deterministic one** — unlike initial placement math, deciding _whether_ to reschedule/skip and by how much depends on unformalizable tradeoffs (how strenuous was the surprise event, how much does today's soreness matter). Deterministic code still executes whatever decision results, but the judgment call itself goes through the Agent/LLM.
- **Detection is split into two paths, not symmetric**:
  - Calendar-detectable changes (a new event appearing) are caught via **periodic polling** — checked on app launch, then roughly every 15–30 minutes while Fairy is running. Deliberately not real-time/webhook-based, since webhooks would require a publicly reachable server (reintroducing hosting costs the project is avoiding).
  - Non-calendar signals (e.g. "I'm not feeling good today") have **no proactive detection path** — these only reach Fairy if the user says so, same as the original daily planning conversation.
- **Confirm-first applies here too**: any reschedule/skip Agent proposes is a suggestion only — nothing changes on the calendar or in the plan until the user confirms, consistent with the calendar-write confirmation pattern already decided in Idea 2.

**Open / unresolved**:

- Exact prompt design and structured output schema not yet defined.
- How agent-computed reminders interact with existing Settings-based intervals (water/break/workout) is not yet designed — likely the Agent's output should override/refine the default interval-based schedule for that day, but this needs explicit design.
- No design yet for what the polling implementation looks like (which calendar APIs get polled, how conflicts are diffed against the existing plan).
- No design yet for the confirm-first UI for reschedule/skip suggestions (likely similar to the calendar-write confirmation UI, but not yet unified or specified).

---

## Idea 2 — Calendar sync

**Concept**: Fairy reads your calendar to know your busy times, and can create new calendar events from what you describe by voice (e.g. saying "date at 4:30pm" creates a real calendar event, and Fairy reminds you at 4:15).

**Calendars involved**:

- User's actual calendar setup is a **genuine mix**: personal events → iCloud, work events → Google, plus other Google-hosted calendars. The iPhone "Calendar app" is just a client showing multiple backend accounts — not a calendar itself.
- Both backends must be supported for **reading** (merging busy/free time across accounts).
- **Google Calendar**: OAuth2 + REST API. Free (no per-call cost within normal personal usage).
- **Apple/iCloud Calendar**: CalDAV protocol (open standard) + app-specific Apple ID password (free, generated at appleid.apple.com). No Apple Developer Program needed — CalDAV is plain HTTP, works from any OS.

**Decisions locked in**:

- **Default write target: iCloud.** New events Fairy creates from voice go to iCloud by default.
- **Write behavior: confirm-first.** Fairy parses your speech into a proposed event (e.g. "date night → iCloud, 4:30pm") and shows it to you — you approve before anything is actually written to your calendar. This was chosen deliberately over silent auto-write, given voice-parsing errors could otherwise corrupt a real calendar; auto-write could be revisited later once parsing is trusted.
- Water/break reminders themselves are **not** written as real calendar events (would be noisy) — they stay as Fairy's own popup reminders, computed around calendar busy time. Only things you explicitly described (like the date) become real calendar entries.

**Architecture**:

- Proposed as a new module: **Calendar Sync**. Owns:
  - OAuth tokens (Google) and CalDAV credentials (iCloud) — should use secure storage (e.g. Tauri's `stronghold` plugin), **not** the existing plaintext `Settings` store, since these are credentials, not preferences.
  - All reads/writes to both calendar backends. No other module talks to Google/iCloud directly.
- Scheduling logic (distributing water breaks into free gaps, deciding to bump the workout to another day) should be **deterministic code**, not an LLM call — more reliable and cheaper. The Agent module extracts structured intent from speech (via the LLM); a plain scheduling algorithm does the constraint-fitting.
- Revised pipeline: Voice → STT → Agent parses day into structured events → Calendar Sync reads existing busy blocks (both backends) → scheduler fits reminders into gaps / decides on workout rescheduling → Calendar Sync writes confirmed events (to iCloud by default) → Behavior fires the popups at computed times.

**Reminder tiers (decided)**:

- **Fairy-only reminders** (water, break): stay as Fairy popups only. Fire only while Fairy is running. Never written to any calendar. Unchanged from v1 behavior.
- **Calendar-backed reminders** (workout, physio sessions, anything with real-world stakes): written to iCloud as real events, so the native Calendar app notifies the user regardless of whether Fairy or the PC is running. When creating these, Calendar Sync must explicitly set an alert/reminder offset on the event (CalDAV `VALARM`, or the equivalent Google Calendar `reminders` field) — otherwise the event exists silently with no notification. This is a required implementation detail, not automatic.
- The reminder's type is what decides which path it takes — no presence detection or manual toggle needed.

**Open / unresolved**:

- No design yet for what the confirm-first UI looks like (a popup? a Settings-UI-style panel?).
- No design yet for CalDAV client implementation specifics (library choice, iCalendar parsing).
- No design yet for how "which calendar" gets exposed as a per-event override in case iCloud isn't the right target for a given event.

---

## Idea 3 — Gaming mode

**Concept**: A manual mode for when the user is playing a game — Fairy's overlay currently sits on top of every program, which isn't appropriate mid-game. Originally considered as automatic fullscreen/game detection, but **deliberately redesigned as a manual toggle** after auditing the automatic approach.

**Why automatic detection was dropped**:

- Fullscreen/game detection is genuinely hard and OS-specific — no reliable cross-platform signal for "a game is running." Would require custom Windows/macOS Rust code (foreground window vs. monitor bounds heuristics, or platform fullscreen APIs), directly contradicting `ARCHITECTURE.md`'s stated assumption that v1 needs no custom Rust.
- Exclusive fullscreen games tend to hide overlay windows regardless of what Fairy does (the OS won't composite Fairy on top) — detection wouldn't even help in that case. Borderless fullscreen windowed (common in modern games) is the case where an overlay can actually still render, and also the easier case to detect — but this nuance made automatic detection feel like a lot of fragile engineering for partial benefit.
- A manual toggle sidesteps all of this: reliable, simple, no platform-specific code.

**Design (decided)**:

- New state field: **`gamingMode: boolean`**, added to `CompanionState` alongside the existing `mode` field. **Owned by Behavior** — same ownership category as `mode`/`activeReminder`/`eye.glowIntensity`, since it governs reminder behavior.
- **Trigger**: a toggle in the **tray icon menu** (Shell already owns the tray) — flipped manually before starting a game, not automatically detected.
- **Shell reacts to `gamingMode` via `state_changed`**, same event-listening pattern Renderer already uses, to reposition the window to a second monitor when available. Shell does not own the field, only reads it — consistent with Shell's existing rule of never touching reminder logic.
- **Behavior changes while `gamingMode` is true**:
  - Voice reminders (once Idea 1's TTS exists) are suppressed — silent, popups only.
  - With a second monitor connected: window moves there, popups still display (silently) but reminders **queue** instead of demanding immediate attention.
  - Without a second monitor: popups are suppressed entirely too — everything just queues, nothing renders during Gaming mode.
- **Clearing the queue**: deliberately **not** a per-reminder dismiss button (would reintroduce the interactivity v1's `VISUAL_SPEC`/instructions explicitly excluded — "no snooze/dismiss buttons"). Instead, **exiting Gaming mode automatically flushes the queue** — each pending reminder plays back-to-back using the existing auto-dismiss behavior. This keeps the "no interactivity" design principle intact while still solving the underlying problem.

**Open / unresolved**:

- Whether `gamingMode` persists across app restarts or always resets to `false` on launch (like the rest of `CompanionState`) hasn't been explicitly decided — current default assumption leans toward resetting, consistent with existing `CompanionState` behavior, but not confirmed.
- Exact queue data structure/ordering when multiple reminder types stack up during a long session not yet designed.
- Tray menu UI/wording for the toggle not yet designed.

## Standing notes

- Both ideas together are a substantial expansion beyond the original v1 scope (lightweight, local-only, no-sound reminder popup app). No roadmap/sequencing against the existing build order (Shell + State/Storage → Behavior + Renderer → Settings UI → Packaging) has been done yet.
- A third idea (custom iOS companion app + push notifications) was explored and **dropped** — Apple's free developer tier has a 7-day app expiry and push notifications (APNs) require the $99/year Apple Developer Program, which didn't fit budget/effort tradeoffs at the time. Not reflected above since it's no longer active, but noting it in case it resurfaces.
