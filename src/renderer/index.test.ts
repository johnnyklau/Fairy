import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CompanionState } from "../state/types";

const invokeMock = vi.fn();
const playBase64WavMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("./audio", () => ({
  playBase64Wav: (...args: unknown[]) => playBase64WavMock(...args),
}));

// Stands in for src-tauri/src/dialogues.rs's FLAVOR_LINES, fetched once at
// startup via getFlavorLines — see docs/VOICE_SPEC.md "Flavor-line voicing".
const DEFAULT_FLAVOR_LINES = [
  "Yes, master?",
  "I'm right here, master.",
  "Boop.",
];

function defaultInvokeImpl(channel: string): Promise<unknown> {
  if (channel === "get_flavor_lines")
    return Promise.resolve(DEFAULT_FLAVOR_LINES);
  return Promise.resolve(undefined);
}

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
    invokeMock.mockReset().mockImplementation(defaultInvokeImpl);
    playBase64WavMock.mockClear();
    document.body.innerHTML = '<div id="app"></div>';
  });

  async function setup() {
    const { initRenderer, renderState } = await import("./index");
    initRenderer();
    // Flavor lines are fetched once, asynchronously, at init — wait for
    // that to land before returning, so click tests don't race an empty
    // local cache (a click while it's still empty is a no-op by design).
    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("get_flavor_lines");
    });
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

    await vi.waitFor(() => {
      expect(
        document.querySelector(".popup")?.classList.contains("visible"),
      ).toBe(true);
    });
    expect(document.querySelector(".popup-text")?.textContent).toBeTruthy();
  });

  it("clicking the eye shows one of the fetched flavor lines", async () => {
    await setup();
    document
      .querySelector(".eye-wrap")
      ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    await vi.waitFor(() => {
      expect(
        document.querySelector(".popup")?.classList.contains("visible"),
      ).toBe(true);
    });
    expect(DEFAULT_FLAVOR_LINES).toContain(
      document.querySelector(".popup-text")?.textContent,
    );
  });

  it("plays audio when activeReminder.audio is present", async () => {
    const { renderState } = await setup();
    renderState(
      stateWith({
        activeReminder: {
          type: "water",
          message: "Time to drink some water, master.",
          triggeredAt: 123,
          audio: { base64Wav: "AAAA", durationMs: 1000 },
        },
      }),
    );
    expect(playBase64WavMock).toHaveBeenCalledWith("AAAA");
  });

  it("does not play audio when activeReminder.audio is absent", async () => {
    const { renderState } = await setup();
    renderState(
      stateWith({
        activeReminder: { type: "water", message: "x", triggeredAt: 123 },
      }),
    );
    expect(playBase64WavMock).not.toHaveBeenCalled();
  });

  it("does not replay audio on a re-render of the same reminder", async () => {
    const { renderState } = await setup();
    const state = stateWith({
      activeReminder: {
        type: "water",
        message: "x",
        triggeredAt: 123,
        audio: { base64Wav: "AAAA", durationMs: 1000 },
      },
    });
    renderState(state);
    renderState(state);
    expect(playBase64WavMock).toHaveBeenCalledTimes(1);
  });

  it("plays a new clip when a second reminder fires with a different triggeredAt", async () => {
    const { renderState } = await setup();
    renderState(
      stateWith({
        activeReminder: {
          type: "water",
          message: "x",
          triggeredAt: 1,
          audio: { base64Wav: "FIRST", durationMs: 1000 },
        },
      }),
    );
    renderState(
      stateWith({
        activeReminder: {
          type: "break",
          message: "y",
          triggeredAt: 2,
          audio: { base64Wav: "SECOND", durationMs: 1000 },
        },
      }),
    );
    expect(playBase64WavMock).toHaveBeenNthCalledWith(1, "FIRST");
    expect(playBase64WavMock).toHaveBeenNthCalledWith(2, "SECOND");
  });

  it("clicking the eye asks the backend to voice the chosen flavor line and waits for it before showing the popup", async () => {
    let resolveSynthesis!: (value: unknown) => void;
    invokeMock.mockImplementation((channel: string) => {
      if (channel === "get_flavor_lines")
        return Promise.resolve(DEFAULT_FLAVOR_LINES);
      if (channel === "synthesize_flavor_line") {
        return new Promise((resolve) => {
          resolveSynthesis = resolve;
        });
      }
      return Promise.resolve(undefined);
    });

    await setup();
    document
      .querySelector(".eye-wrap")
      ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    // Still pending synthesis — popup must not be visible yet.
    expect(
      document.querySelector(".popup")?.classList.contains("visible"),
    ).toBe(false);
    expect(invokeMock).toHaveBeenCalledWith("synthesize_flavor_line", {
      index: expect.any(Number),
    });

    resolveSynthesis({ base64Wav: "FLAVORWAV", durationMs: 2000 });

    await vi.waitFor(() => {
      expect(
        document.querySelector(".popup")?.classList.contains("visible"),
      ).toBe(true);
    });
    expect(playBase64WavMock).toHaveBeenCalledWith(
      "FLAVORWAV",
      expect.any(Function),
    );
  });

  it("dismisses the flavor popup when the audio's real ended event fires, not just the duration estimate", async () => {
    invokeMock.mockImplementation((channel: string) => {
      if (channel === "get_flavor_lines")
        return Promise.resolve(DEFAULT_FLAVOR_LINES);
      if (channel === "synthesize_flavor_line") {
        // A (deliberately wrong) huge duration estimate — if dismissal
        // relied on this alone, the popup would still be visible when we
        // check below. It shouldn't, because the real `ended` callback
        // fires first.
        return Promise.resolve({ base64Wav: "FLAVORWAV", durationMs: 60_000 });
      }
      return Promise.resolve(undefined);
    });

    await setup();
    document
      .querySelector(".eye-wrap")
      ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    await vi.waitFor(() => {
      expect(
        document.querySelector(".popup")?.classList.contains("visible"),
      ).toBe(true);
    });

    const onEnded = playBase64WavMock.mock.calls[0][1] as () => void;
    onEnded();

    await vi.waitFor(() => {
      expect(
        document.querySelector(".popup")?.classList.contains("visible"),
      ).toBe(false);
    });
  });

  it("tells Shell the flavor popup is visible while shown, and clears it once dismissed", async () => {
    invokeMock.mockImplementation((channel: string) => {
      if (channel === "get_flavor_lines")
        return Promise.resolve(DEFAULT_FLAVOR_LINES);
      if (channel === "synthesize_flavor_line") {
        return Promise.resolve({ base64Wav: "FLAVORWAV", durationMs: 100 });
      }
      return Promise.resolve(undefined);
    });

    await setup();
    document
      .querySelector(".eye-wrap")
      ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("set_flavor_popup_visible", {
        visible: true,
      });
    });

    const onEnded = playBase64WavMock.mock.calls[0][1] as () => void;
    onEnded();

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("set_flavor_popup_visible", {
        visible: false,
      });
    });
  });
});
