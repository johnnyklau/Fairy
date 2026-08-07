import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CompanionState } from "./types";

const invokeMock = vi.fn();
const listenMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));

const sampleState: CompanionState = {
  mode: "idle",
  activeReminder: null,
  eye: { glowIntensity: "low" },
  window: { clickThrough: true, corner: "top-right" },
};

function flush(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

describe("state/initState", () => {
  beforeEach(() => {
    vi.resetModules();
    invokeMock.mockReset();
    listenMock.mockReset();
  });

  it("fetches an initial snapshot via get_state on load", async () => {
    invokeMock.mockResolvedValue(sampleState);
    listenMock.mockResolvedValue(() => {});

    const { initState } = await import("./index");
    const onChange = vi.fn();
    initState(onChange);
    await flush();

    expect(invokeMock).toHaveBeenCalledWith("get_state");
    expect(onChange).toHaveBeenCalledWith(sampleState);
  });

  it("subscribes to state_changed and forwards future events to onChange", async () => {
    invokeMock.mockResolvedValue(sampleState);
    let capturedCallback:
      ((event: { payload: CompanionState }) => void) | undefined;
    listenMock.mockImplementation(
      (_channel: string, cb: (event: { payload: CompanionState }) => void) => {
        capturedCallback = cb;
        return Promise.resolve(() => {});
      },
    );

    const { initState } = await import("./index");
    const onChange = vi.fn();
    initState(onChange);
    await flush();

    expect(listenMock).toHaveBeenCalledWith(
      "state_changed",
      expect.any(Function),
    );

    const updated: CompanionState = {
      ...sampleState,
      eye: { glowIntensity: "high" },
    };
    capturedCallback?.({ payload: updated });

    expect(onChange).toHaveBeenCalledWith(updated);
  });

  it("does not require the snapshot fetch to resolve before subscribing", async () => {
    // The snapshot fetch (invoke) and the live subscription (listen) are
    // independent — a slow/never-resolving get_state must not block
    // state_changed events from being wired up.
    invokeMock.mockReturnValue(new Promise(() => {})); // never resolves
    listenMock.mockResolvedValue(() => {});

    const { initState } = await import("./index");
    const onChange = vi.fn();
    initState(onChange);
    await flush();

    expect(listenMock).toHaveBeenCalled();
  });
});
