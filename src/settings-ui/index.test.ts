import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MonitorInfo, Settings } from "../state/types";

const defaultSettings: Settings = {
  water: { enabled: true, intervalMinutes: 60 },
  breakReminder: { enabled: true, intervalMinutes: 50 },
  workout: { enabled: true, timeOfDay: "18:00" },
  idleBark: { enabled: false },
  position: { corner: "top-right", monitorIndex: 0 },
  autostart: { enabled: true },
};

const defaultMonitors: MonitorInfo[] = [
  { index: 0, name: "Monitor 1", width: 1920, height: 1080 },
];

const getSettingsMock = vi.fn();
const updateSettingsMock = vi.fn();
const listMonitorsMock = vi.fn();
const closeSettingsMock = vi.fn();

vi.mock("../state", () => ({
  getSettings: (...args: unknown[]) => getSettingsMock(...args),
  updateSettings: (...args: unknown[]) => updateSettingsMock(...args),
  listMonitors: (...args: unknown[]) => listMonitorsMock(...args),
  closeSettings: (...args: unknown[]) => closeSettingsMock(...args),
}));

describe("settings-ui", () => {
  beforeEach(() => {
    vi.resetModules();
    getSettingsMock.mockReset().mockResolvedValue(defaultSettings);
    updateSettingsMock.mockReset().mockResolvedValue(defaultSettings);
    listMonitorsMock.mockReset().mockResolvedValue(defaultMonitors);
    closeSettingsMock.mockReset().mockResolvedValue(undefined);
    document.body.innerHTML = '<div id="settings-app"></div>';
  });

  async function setup() {
    const { initSettingsUi } = await import("./index");
    initSettingsUi();
    // Wait for the populateMonitors().then(() => getSettings().then(applySettings))
    // chain to actually settle, rather than gambling on a fixed number of
    // flushed microtask/macrotask ticks being "enough" (timing-fragile
    // across environments/runners).
    await vi.waitFor(() => {
      expect(
        document.querySelector<HTMLInputElement>('[name="waterEnabled"]')
          ?.checked,
      ).toBe(defaultSettings.water.enabled);
      expect(
        document.querySelectorAll('[name="monitorIndex"] option'),
      ).toHaveLength(defaultMonitors.length);
    });
  }

  it("populates form fields from fetched settings", async () => {
    await setup();
    expect(
      document.querySelector<HTMLInputElement>('[name="waterEnabled"]')
        ?.checked,
    ).toBe(true);
    expect(
      document.querySelector<HTMLInputElement>('[name="waterInterval"]')?.value,
    ).toBe("60");
    expect(
      document.querySelector<HTMLInputElement>('[name="workoutTime"]')?.value,
    ).toBe("18:00");
    expect(
      document.querySelector<HTMLSelectElement>('[name="corner"]')?.value,
    ).toBe("top-right");
  });

  it("populates the monitor dropdown from listMonitors", async () => {
    await setup();
    const options = document.querySelectorAll<HTMLOptionElement>(
      '[name="monitorIndex"] option',
    );
    expect(options).toHaveLength(1);
    expect(options[0].textContent).toContain("1920x1080");
  });

  it("sends the full water group (not just the changed field) on change", async () => {
    await setup();
    const waterEnabled = document.querySelector<HTMLInputElement>(
      '[name="waterEnabled"]',
    )!;
    waterEnabled.checked = false;
    waterEnabled.dispatchEvent(new Event("change"));

    expect(updateSettingsMock).toHaveBeenCalledWith({
      water: { enabled: false, intervalMinutes: 60 },
    });
  });

  it("sends an autostart patch when the startup toggle changes", async () => {
    await setup();
    const autostartEnabled = document.querySelector<HTMLInputElement>(
      '[name="autostartEnabled"]',
    )!;
    autostartEnabled.checked = false;
    autostartEnabled.dispatchEvent(new Event("change"));

    expect(updateSettingsMock).toHaveBeenCalledWith({
      autostart: { enabled: false },
    });
  });

  it("sends both workout fields together when only time changes", async () => {
    await setup();
    const workoutTime = document.querySelector<HTMLInputElement>(
      '[name="workoutTime"]',
    )!;
    workoutTime.value = "07:30";
    workoutTime.dispatchEvent(new Event("change"));

    expect(updateSettingsMock).toHaveBeenCalledWith({
      workout: { enabled: true, timeOfDay: "07:30" },
    });
  });

  it("sends position patch with both corner and monitorIndex on corner change", async () => {
    await setup();
    const corner =
      document.querySelector<HTMLSelectElement>('[name="corner"]')!;
    corner.value = "bottom-left";
    corner.dispatchEvent(new Event("change"));

    expect(updateSettingsMock).toHaveBeenCalledWith({
      position: { corner: "bottom-left", monitorIndex: 0 },
    });
  });

  describe("when updateSettings rejects", () => {
    let consoleErrorSpy: ReturnType<typeof vi.spyOn>;

    beforeEach(() => {
      consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    });

    afterEach(() => {
      consoleErrorSpy.mockRestore();
    });

    it("re-fetches settings to re-sync the form instead of leaving a stale value", async () => {
      await setup();
      updateSettingsMock.mockRejectedValueOnce(
        new Error("invalid workout time"),
      );

      const workoutTime = document.querySelector<HTMLInputElement>(
        '[name="workoutTime"]',
      )!;
      workoutTime.value = "bad-value";
      workoutTime.dispatchEvent(new Event("change"));

      // Once on initial load, once more to re-sync after the rejection.
      await vi.waitFor(() => {
        expect(getSettingsMock).toHaveBeenCalledTimes(2);
      });
    });
  });

  it("close button calls closeSettings", async () => {
    await setup();
    document
      .querySelector(".close-button")
      ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    expect(closeSettingsMock).toHaveBeenCalled();
  });
});
