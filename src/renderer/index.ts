import { reportEyeBounds } from "../state";
import type { CompanionState } from "../state/types";
import { createEyeElement, setEyeGlowHigh } from "./eye";
import { createPopupElement, hidePopup, setPopupText, showPopup } from "./popup";

// Flavor text for clicking the eye — fluff/testing aid, not a real reminder.
// Purely local to the Renderer; doesn't touch backend state.
const FLAVOR_LINES = [
  "Yes, master?",
  "I'm right here, master.",
  "Did you need something, master?",
  "Boop.",
  "...watching you work, master.",
];
const FLAVOR_DISPLAY_MS = 4000;

let mounted = false;
let container: HTMLDivElement;
let eyeWrap: HTMLDivElement;
let eyeEl: SVGSVGElement;
let popupEl: HTMLDivElement;
let flavorTimeout: ReturnType<typeof setTimeout> | undefined;

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
    const line = FLAVOR_LINES[Math.floor(Math.random() * FLAVOR_LINES.length)];
    setPopupText(popupEl, line);
    showPopup(popupEl);
    clearTimeout(flavorTimeout);
    flavorTimeout = setTimeout(() => hidePopup(popupEl), FLAVOR_DISPLAY_MS);
  });

  reportBounds();
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
    setPopupText(popupEl, state.activeReminder.message);
    showPopup(popupEl);
  } else {
    hidePopup(popupEl);
  }
}
