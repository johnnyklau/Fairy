import { closeSettings, getSettings, listMonitors, updateSettings } from "../state";
import type { Settings } from "../state/types";

export function initSettingsUi(): void {
  const root = document.getElementById("settings-app");
  if (!root) return;

  root.innerHTML = `
    <form class="settings-form">
      <fieldset>
        <legend>Water</legend>
        <label><input type="checkbox" name="waterEnabled" /> Enabled</label>
        <label>Interval (minutes)
          <input type="number" name="waterInterval" min="1" />
        </label>
      </fieldset>

      <fieldset>
        <legend>Break</legend>
        <label><input type="checkbox" name="breakEnabled" /> Enabled</label>
        <label>Interval (minutes)
          <input type="number" name="breakInterval" min="1" />
        </label>
      </fieldset>

      <fieldset>
        <legend>Workout</legend>
        <label><input type="checkbox" name="workoutEnabled" /> Enabled</label>
        <label>Time of day
          <input type="time" name="workoutTime" />
        </label>
      </fieldset>

      <fieldset>
        <legend>Idle bark</legend>
        <label><input type="checkbox" name="idleBarkEnabled" /> Enabled</label>
      </fieldset>

      <fieldset>
        <legend>Position</legend>
        <label>Monitor
          <select name="monitorIndex"></select>
        </label>
        <label>Corner
          <select name="corner">
            <option value="top-left">Top left</option>
            <option value="top-right">Top right</option>
            <option value="bottom-left">Bottom left</option>
            <option value="bottom-right">Bottom right</option>
          </select>
        </label>
      </fieldset>

      <button type="button" class="close-button">Close</button>
    </form>
  `;

  const form = root.querySelector<HTMLFormElement>(".settings-form");
  if (!form) return;

  const fields = {
    waterEnabled: form.elements.namedItem("waterEnabled") as HTMLInputElement,
    waterInterval: form.elements.namedItem(
      "waterInterval",
    ) as HTMLInputElement,
    breakEnabled: form.elements.namedItem("breakEnabled") as HTMLInputElement,
    breakInterval: form.elements.namedItem(
      "breakInterval",
    ) as HTMLInputElement,
    workoutEnabled: form.elements.namedItem(
      "workoutEnabled",
    ) as HTMLInputElement,
    workoutTime: form.elements.namedItem("workoutTime") as HTMLInputElement,
    idleBarkEnabled: form.elements.namedItem(
      "idleBarkEnabled",
    ) as HTMLInputElement,
    monitorIndex: form.elements.namedItem(
      "monitorIndex",
    ) as HTMLSelectElement,
    corner: form.elements.namedItem("corner") as HTMLSelectElement,
  };

  function applySettings(settings: Settings): void {
    fields.waterEnabled.checked = settings.water.enabled;
    fields.waterInterval.value = String(settings.water.intervalMinutes);
    fields.breakEnabled.checked = settings.breakReminder.enabled;
    fields.breakInterval.value = String(
      settings.breakReminder.intervalMinutes,
    );
    fields.workoutEnabled.checked = settings.workout.enabled;
    fields.workoutTime.value = settings.workout.timeOfDay;
    fields.idleBarkEnabled.checked = settings.idleBark.enabled;
    fields.corner.value = settings.position.corner;
    fields.monitorIndex.value = String(settings.position.monitorIndex);
  }

  function handleChange(patch: Partial<Settings>): void {
    void updateSettings(patch)
      .then(applySettings)
      .catch((error: unknown) => {
        // Rejected (e.g. invalid workout time) — re-sync the form to
        // what's actually persisted rather than leaving a stale/unsaved
        // value displayed.
        console.error("updateSettings failed:", error);
        void getSettings().then(applySettings);
      });
  }

  async function populateMonitors(): Promise<void> {
    const monitors = await listMonitors();
    fields.monitorIndex.innerHTML = "";
    for (const monitor of monitors) {
      const option = document.createElement("option");
      option.value = String(monitor.index);
      option.textContent = `${monitor.name ?? `Monitor ${monitor.index + 1}`} (${monitor.width}x${monitor.height})`;
      fields.monitorIndex.appendChild(option);
    }
  }

  void populateMonitors().then(() => getSettings().then(applySettings));

  fields.waterEnabled.addEventListener("change", () =>
    handleChange({
      water: {
        enabled: fields.waterEnabled.checked,
        intervalMinutes: Number(fields.waterInterval.value),
      },
    }),
  );
  fields.waterInterval.addEventListener("change", () =>
    handleChange({
      water: {
        enabled: fields.waterEnabled.checked,
        intervalMinutes: Number(fields.waterInterval.value),
      },
    }),
  );
  fields.breakEnabled.addEventListener("change", () =>
    handleChange({
      breakReminder: {
        enabled: fields.breakEnabled.checked,
        intervalMinutes: Number(fields.breakInterval.value),
      },
    }),
  );
  fields.breakInterval.addEventListener("change", () =>
    handleChange({
      breakReminder: {
        enabled: fields.breakEnabled.checked,
        intervalMinutes: Number(fields.breakInterval.value),
      },
    }),
  );
  fields.workoutEnabled.addEventListener("change", () =>
    handleChange({
      workout: {
        enabled: fields.workoutEnabled.checked,
        timeOfDay: fields.workoutTime.value,
      },
    }),
  );
  fields.workoutTime.addEventListener("change", () =>
    handleChange({
      workout: {
        enabled: fields.workoutEnabled.checked,
        timeOfDay: fields.workoutTime.value,
      },
    }),
  );
  fields.idleBarkEnabled.addEventListener("change", () =>
    handleChange({ idleBark: { enabled: fields.idleBarkEnabled.checked } }),
  );
  fields.monitorIndex.addEventListener("change", () =>
    handleChange({
      position: {
        corner: fields.corner.value as Settings["position"]["corner"],
        monitorIndex: Number(fields.monitorIndex.value),
      },
    }),
  );
  fields.corner.addEventListener("change", () =>
    handleChange({
      position: {
        corner: fields.corner.value as Settings["position"]["corner"],
        monitorIndex: Number(fields.monitorIndex.value),
      },
    }),
  );

  form.querySelector(".close-button")?.addEventListener("click", () => {
    void closeSettings();
  });
}
