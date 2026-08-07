import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CompanionState } from "../state/types";

const invokeMock = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function stateWith(overrides: Partial<CompanionState>): CompanionState {
  return {
    mode: "idle",
    activeReminder: null,
    eye: { glowIntensity: "low" },
    window: { clickThrough: true, corner: "top-left" },
    ...overrides,
  };
}

describe("renderer", () => {
  beforeEach(() => {
    vi.resetModules();
    invokeMock.mockClear();
    document.body.innerHTML = '<div id="app"></div>';
  });

  async function setup() {
    const { initRenderer, renderState } = await import("./index");
    initRenderer();
    return { renderState };
  }

  it("mounts the eye and popup into #app", async () => {
    await setup();
    expect(document.querySelector(".eye-wrap")).not.toBeNull();
    expect(document.querySelector(".popup")).not.toBeNull();
  });

  it("adds glow-high when glowIntensity is high", async () => {
    const { renderState } = await setup();
    renderState(stateWith({ eye: { glowIntensity: "high" } }));
    expect(
      document.querySelector(".eye")?.classList.contains("glow-high"),
    ).toBe(true);
  });

  it("removes glow-high when glowIntensity is low", async () => {
    const { renderState } = await setup();
    renderState(stateWith({ eye: { glowIntensity: "high" } }));
    renderState(stateWith({ eye: { glowIntensity: "low" } }));
    expect(
      document.querySelector(".eye")?.classList.contains("glow-high"),
    ).toBe(false);
  });

  it("shows the popup with the reminder message when activeReminder is set", async () => {
    const { renderState } = await setup();
    renderState(
      stateWith({
        activeReminder: {
          type: "water",
          message: "Time to drink some water, master.",
          triggeredAt: 0,
        },
      }),
    );
    expect(
      document.querySelector(".popup")?.classList.contains("visible"),
    ).toBe(true);
    expect(document.querySelector(".popup-text")?.textContent).toBe(
      "Time to drink some water, master.",
    );
  });

  it("hides the popup when activeReminder is null", async () => {
    const { renderState } = await setup();
    renderState(
      stateWith({
        activeReminder: { type: "water", message: "x", triggeredAt: 0 },
      }),
    );
    renderState(stateWith({ activeReminder: null }));
    expect(
      document.querySelector(".popup")?.classList.contains("visible"),
    ).toBe(false);
  });

  it("anchors right for top-right and bottom-right corners", async () => {
    const { renderState } = await setup();
    renderState(
      stateWith({ window: { clickThrough: true, corner: "top-right" } }),
    );
    expect(
      document.querySelector(".companion")?.classList.contains("anchor-right"),
    ).toBe(true);

    renderState(
      stateWith({ window: { clickThrough: true, corner: "bottom-right" } }),
    );
    expect(
      document.querySelector(".companion")?.classList.contains("anchor-right"),
    ).toBe(true);
  });

  it("does not anchor right for top-left and bottom-left corners", async () => {
    const { renderState } = await setup();
    renderState(
      stateWith({ window: { clickThrough: true, corner: "top-left" } }),
    );
    expect(
      document.querySelector(".companion")?.classList.contains("anchor-right"),
    ).toBe(false);

    renderState(
      stateWith({ window: { clickThrough: true, corner: "bottom-left" } }),
    );
    expect(
      document.querySelector(".companion")?.classList.contains("anchor-right"),
    ).toBe(false);
  });

  it("reports eye bounds to Shell via invoke on mount and on every renderState", async () => {
    invokeMock.mockClear();
    const { renderState } = await setup();
    expect(invokeMock).toHaveBeenCalledWith(
      "report_eye_bounds",
      expect.objectContaining({ bounds: expect.any(Object) }),
    );

    invokeMock.mockClear();
    renderState(stateWith({}));
    expect(invokeMock).toHaveBeenCalledWith(
      "report_eye_bounds",
      expect.objectContaining({ bounds: expect.any(Object) }),
    );
  });

  it("clicking the eye shows a flavor-text popup, independent of backend state", async () => {
    await setup();
    document
      .querySelector(".eye-wrap")
      ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    expect(
      document.querySelector(".popup")?.classList.contains("visible"),
    ).toBe(true);
    expect(document.querySelector(".popup-text")?.textContent).toBeTruthy();
  });
});
