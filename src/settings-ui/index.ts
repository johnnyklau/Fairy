import { listen } from "@tauri-apps/api/event";
import {
  closeSettings,
  getSettings,
  listMonitors,
  updateSettings,
} from "../state";
import type { Settings } from "../state/types";

interface VoiceModelDownloadProgress {
  downloadedBytes: number;
  totalBytes: number;
}

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
        <legend>Startup</legend>
        <label><input type="checkbox" name="autostartEnabled" /> Launch Fairy when Windows starts</label>
      </fieldset>

      <fieldset>
        <legend>Voice</legend>
        <label><input type="checkbox" name="voiceEnabled" /> Enabled</label>
        <label>Language
          <select name="voiceLanguage">
            <option value="en">English</option>
            <option value="ja">Japanese</option>
          </select>
        </label>
        <label>Volume
          <input type="range" name="voiceVolume" min="0" max="1" step="0.01" />
        </label>
        <p class="voice-download-progress" hidden></p>
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
    waterInterval: form.elements.namedItem("waterInterval") as HTMLInputElement,
    breakEnabled: form.elements.namedItem("breakEnabled") as HTMLInputElement,
    breakInterval: form.elements.namedItem("breakInterval") as HTMLInputElement,
    workoutEnabled: form.elements.namedItem(
      "workoutEnabled",
    ) as HTMLInputElement,
    workoutTime: form.elements.namedItem("workoutTime") as HTMLInputElement,
    idleBarkEnabled: form.elements.namedItem(
      "idleBarkEnabled",
    ) as HTMLInputElement,
    autostartEnabled: form.elements.namedItem(
      "autostartEnabled",
    ) as HTMLInputElement,
    monitorIndex: form.elements.namedItem("monitorIndex") as HTMLSelectElement,
    corner: form.elements.namedItem("corner") as HTMLSelectElement,
    voiceEnabled: form.elements.namedItem("voiceEnabled") as HTMLInputElement,
    voiceLanguage: form.elements.namedItem(
      "voiceLanguage",
    ) as HTMLSelectElement,
    voiceVolume: form.elements.namedItem("voiceVolume") as HTMLInputElement,
  };
  const voiceDownloadProgress = form.querySelector<HTMLParagraphElement>(
    ".voice-download-progress",
  );

  function applySettings(settings: Settings): void {
    fields.waterEnabled.checked = settings.water.enabled;
    fields.waterInterval.value = String(settings.water.intervalMinutes);
    fields.breakEnabled.checked = settings.breakReminder.enabled;
    fields.breakInterval.value = String(settings.breakReminder.intervalMinutes);
    fields.workoutEnabled.checked = settings.workout.enabled;
    fields.workoutTime.value = settings.workout.timeOfDay;
    fields.idleBarkEnabled.checked = settings.idleBark.enabled;
    fields.autostartEnabled.checked = settings.autostart.enabled;
    fields.corner.value = settings.position.corner;
    fields.monitorIndex.value = String(settings.position.monitorIndex);
    fields.voiceEnabled.checked = settings.voice.enabled;
    fields.voiceLanguage.value = settings.voice.language;
    fields.voiceVolume.value = String(settings.voice.volume);
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
  fields.autostartEnabled.addEventListener("change", () =>
    handleChange({ autostart: { enabled: fields.autostartEnabled.checked } }),
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

  function voicePatch(): Partial<Settings> {
    return {
      voice: {
        enabled: fields.voiceEnabled.checked,
        language: fields.voiceLanguage.value as Settings["voice"]["language"],
        volume: Number(fields.voiceVolume.value),
      },
    };
  }
  fields.voiceEnabled.addEventListener("change", () =>
    handleChange(voicePatch()),
  );
  fields.voiceLanguage.addEventListener("change", () =>
    handleChange(voicePatch()),
  );
  fields.voiceVolume.addEventListener("change", () =>
    handleChange(voicePatch()),
  );

  // Backend fires this while downloading the voice model after the user
  // first enables Voice (see voice::download_with_progress in
  // src-tauri/src/voice.rs). Only relevant while a download is in
  // flight — the element stays hidden the rest of the time.
  void listen<VoiceModelDownloadProgress>(
    "voice_model_download_progress",
    (event) => {
      if (!voiceDownloadProgress) return;
      const { downloadedBytes, totalBytes } = event.payload;
      voiceDownloadProgress.hidden = false;
      if (totalBytes > 0) {
        const percent = Math.round((downloadedBytes / totalBytes) * 100);
        voiceDownloadProgress.textContent = `Downloading voice model… ${percent}%`;
        if (downloadedBytes >= totalBytes) {
          voiceDownloadProgress.hidden = true;
        }
      } else {
        const mb = (downloadedBytes / (1024 * 1024)).toFixed(1);
        voiceDownloadProgress.textContent = `Downloading voice model… ${mb} MB`;
      }
    },
  );

  form.querySelector(".close-button")?.addEventListener("click", () => {
    void closeSettings();
  });
}
