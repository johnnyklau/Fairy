import {
  getFlavorLines,
  installUpdate,
  reportEyeBounds,
  setFlavorPopupVisible,
  synthesizeFlavorLine,
} from "../state";
import type { CompanionState } from "../state/types";
import { playBase64Wav } from "./audio";
import { createEyeElement, setEyeGlowHigh } from "./eye";
import {
  createPopupElement,
  hidePopup,
  setPopupText,
  showPopup,
} from "./popup";

// Floor for text-only display (voice off, or synthesis failed/disabled) —
// generous enough to comfortably read any current flavor line without
// timing it precisely to length.
const FLAVOR_DISPLAY_MS = 6500;
// Mirrors Behavior's AUDIO_TAIL_BUFFER in src-tauri/src/behavior.rs — keep
// both in sync if either changes.
const AUDIO_TAIL_BUFFER_MS = 500;
// How long the "update available" popup stays up before giving up for this
// launch — longer than a flavor line's display time since this is a
// decision, not a one-liner to skim. Missing it isn't a big deal: Fairy
// asks again next launch, not on a repeating timer within a session.
const UPDATE_PROMPT_DISPLAY_MS = 20000;

let mounted = false;
let container: HTMLDivElement;
let eyeWrap: HTMLDivElement;
let eyeEl: SVGSVGElement;
let popupEl: HTMLDivElement;
let flavorTimeout: ReturnType<typeof setTimeout> | undefined;
let lastPlayedReminderAt: number | null = null;
// Whether the one-time update-available popup has already been shown this
// launch (never re-shown even if it times out unclicked — see
// UPDATE_PROMPT_DISPLAY_MS) and whether it's currently on screen, which is
// what the eye-click handler checks to decide "install the update" vs. the
// usual "show a flavor line".
let updatePromptShown = false;
let updatePromptActive = false;
let updateHideTimeout: ReturnType<typeof setTimeout> | undefined;
// Fetched once at startup from src-tauri/src/dialogues.rs (the single
// source of truth for dialogue content) via getFlavorLines, then picked
// from locally on every click — same instant response as a hardcoded
// array, just sourced from the backend instead of duplicated here.
let flavorLines: string[] = [];

export function initRenderer(): void {
  if (mounted) return;
  mounted = true;

  const root = document.getElementById("app");
  if (!root) return;

  container = document.createElement("div");
  container.className = "companion";

  eyeWrap = document.createElement("div");
  eyeWrap.className = "eye-wrap";
  eyeEl = createEyeElement();
  eyeWrap.appendChild(eyeEl);

  popupEl = createPopupElement();

  container.appendChild(eyeWrap);
  container.appendChild(popupEl);
  root.appendChild(container);

  eyeWrap.addEventListener("click", () => {
    if (updatePromptActive) {
      void handleUpdateClick();
      return;
    }
    void handleFlavorClick();
  });

  void getFlavorLines().then((lines) => {
    flavorLines = lines;
  });

  reportBounds();
}

// Waits for voiced audio (if voice is enabled) before showing the popup —
// mirrors the "popup waits for audio" choice made for reminders, so a
// click never shows English text while Japanese audio trails in half a
// second later out of sync. When voice is disabled, synthesizeFlavorLine
// resolves to null via one cheap IPC round-trip, not a synthesis wait.
async function handleFlavorClick(): Promise<void> {
  if (flavorLines.length === 0) return; // not fetched yet — click is a no-op
  const index = Math.floor(Math.random() * flavorLines.length);
  const line = flavorLines[index];
  const audio = await synthesizeFlavorLine(index).catch(() => null);

  setPopupText(popupEl, line);
  showPopup(popupEl);
  void setFlavorPopupVisible(true);

  const hide = () => {
    hidePopup(popupEl);
    void setFlavorPopupVisible(false);
  };

  clearTimeout(flavorTimeout);
  if (audio) {
    // duration_ms is an estimate up front — this timer is just a safety
    // net (e.g. if playback fails to start silently). The real dismissal
    // below fires off the audio element's actual `ended` event, so the
    // popup always lasts exactly until the dialogue finishes playing.
    const fallbackMs = audio.durationMs + AUDIO_TAIL_BUFFER_MS;
    flavorTimeout = setTimeout(hide, fallbackMs);
    playBase64Wav(audio.base64Wav, () => {
      clearTimeout(flavorTimeout);
      flavorTimeout = setTimeout(hide, AUDIO_TAIL_BUFFER_MS);
    });
  } else {
    flavorTimeout = setTimeout(hide, FLAVOR_DISPLAY_MS);
  }
}

// Fires instead of handleFlavorClick while the update-available popup is
// showing (see renderState). Install failures (no network, bad signature)
// degrade to a brief message rather than leaving "Updating…" up forever —
// same "fail quietly" contract voice synthesis already follows. On success
// there's nothing further to do here: src-tauri/src/updater.rs's
// install_update relaunches the whole process into the new version.
async function handleUpdateClick(): Promise<void> {
  updatePromptActive = false;
  clearTimeout(updateHideTimeout);
  setPopupText(popupEl, "Updating…");

  try {
    await installUpdate();
  } catch {
    setPopupText(popupEl, "Update failed — try again next launch.");
    setTimeout(() => hidePopup(popupEl), FLAVOR_DISPLAY_MS);
  }
}

// The only source of truth Shell uses for hover/click-through hit-testing —
// see report_eye_bounds in src-tauri/src/shell.rs. Re-report whenever
// layout could have moved the eye (e.g. anchor-right toggling), so a CSS
// change alone is always reflected, never hardcoded on the Rust side.
function reportBounds(): void {
  const rect = eyeWrap.getBoundingClientRect();
  void reportEyeBounds(rect.left, rect.top, rect.width, rect.height);
}

export function renderState(state: CompanionState): void {
  if (!mounted) return;

  const anchorRight =
    state.window.corner === "top-right" ||
    state.window.corner === "bottom-right";
  container.classList.toggle("anchor-right", anchorRight);
  reportBounds();

  setEyeGlowHigh(eyeEl, state.eye.glowIntensity === "high");

  if (state.activeReminder) {
    // Reminders always take the popup over an update prompt — if one was
    // showing, drop it; clicking now falls back to the usual flavor-line
    // behavior rather than silently doing nothing.
    updatePromptActive = false;
    clearTimeout(updateHideTimeout);

    setPopupText(popupEl, state.activeReminder.message);
    showPopup(popupEl);
    // renderState can be called repeatedly for the same active reminder
    // (e.g. a corner change re-render) — only play audio once per
    // reminder, keyed by triggeredAt, not on every call.
    if (
      state.activeReminder.audio &&
      state.activeReminder.triggeredAt !== lastPlayedReminderAt
    ) {
      lastPlayedReminderAt = state.activeReminder.triggeredAt;
      playBase64Wav(state.activeReminder.audio.base64Wav);
    }
    return;
  }

  if (state.updateAvailable && !updatePromptShown) {
    updatePromptShown = true;
    updatePromptActive = true;
    setPopupText(
      popupEl,
      `A new version of Fairy is available (v${state.updateAvailable.version}). Click to update.`,
    );
    showPopup(popupEl);
    updateHideTimeout = setTimeout(() => {
      updatePromptActive = false;
      hidePopup(popupEl);
    }, UPDATE_PROMPT_DISPLAY_MS);
    return;
  }

  if (!updatePromptActive) {
    hidePopup(popupEl);
  }
}
