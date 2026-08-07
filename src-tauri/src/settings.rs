use crate::shell::{apply_autostart, apply_window_position};
use crate::state::Settings;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "settings.json";
const SETTINGS_KEY: &str = "settings";
const MIN_INTERVAL_MINUTES: u32 = 1;
const MAX_INTERVAL_MINUTES: u32 = 24 * 60;

/// Clamps/repairs fields into a sane range. Applied on every load (not just
/// write) as defense-in-depth — e.g. an `intervalMinutes: 0` that somehow
/// got persisted would otherwise fire on every 15s scheduler tick forever,
/// and a hand-edited malformed `timeOfDay` would otherwise just silently
/// never match and never fire again.
fn sanitize(settings: &mut Settings) {
    settings.water.interval_minutes = settings
        .water
        .interval_minutes
        .clamp(MIN_INTERVAL_MINUTES, MAX_INTERVAL_MINUTES);
    settings.break_reminder.interval_minutes = settings
        .break_reminder
        .interval_minutes
        .clamp(MIN_INTERVAL_MINUTES, MAX_INTERVAL_MINUTES);
    if !is_valid_time_of_day(&settings.workout.time_of_day) {
        settings.workout.time_of_day = Settings::default().workout.time_of_day;
    }
}

fn is_valid_time_of_day(value: &str) -> bool {
    let Some((hours, minutes)) = value.split_once(':') else {
        return false;
    };
    if hours.len() != 2 || minutes.len() != 2 {
        return false;
    }
    let (Ok(h), Ok(m)) = (hours.parse::<u32>(), minutes.parse::<u32>()) else {
        return false;
    };
    h < 24 && m < 60
}

pub fn load_settings(app: &AppHandle) -> Settings {
    let Ok(store) = app.store(STORE_FILE) else {
        return Settings::default();
    };
    let mut settings: Settings = store
        .get(SETTINGS_KEY)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    sanitize(&mut settings);
    settings
}

fn merge_json(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(patch_map)) => {
            for (key, patch_value) in patch_map {
                merge_json(
                    base_map
                        .entry(key.clone())
                        .or_insert(serde_json::Value::Null),
                    patch_value,
                );
            }
        }
        (base_slot, patch_value) => {
            *base_slot = patch_value.clone();
        }
    }
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    load_settings(&app)
}

#[tauri::command]
pub fn update_settings(app: AppHandle, settings: serde_json::Value) -> Result<Settings, String> {
    let current = load_settings(&app);
    let mut merged_json = serde_json::to_value(&current).map_err(|e| e.to_string())?;
    merge_json(&mut merged_json, &settings);
    let mut merged: Settings = serde_json::from_value(merged_json).map_err(|e| e.to_string())?;

    if !is_valid_time_of_day(&merged.workout.time_of_day) {
        return Err(format!(
            "invalid workout time_of_day {:?}, expected 24-hour HH:MM",
            merged.workout.time_of_day
        ));
    }
    sanitize(&mut merged);

    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    let value = serde_json::to_value(&merged).map_err(|e| e.to_string())?;
    store.set(SETTINGS_KEY, value);
    store.save().map_err(|e| e.to_string())?;

    apply_window_position(&app, &merged.position);
    apply_autostart(&app, merged.autostart.enabled);

    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ScreenCorner;
    use serde_json::json;

    #[test]
    fn merge_partial_water_patch_leaves_other_fields_untouched() {
        let mut base = serde_json::to_value(Settings::default()).unwrap();
        let patch = json!({ "water": { "enabled": false, "intervalMinutes": 90 } });

        merge_json(&mut base, &patch);
        let merged: Settings = serde_json::from_value(base).unwrap();

        assert!(!merged.water.enabled);
        assert_eq!(merged.water.interval_minutes, 90);
        // Untouched by the patch — must survive the merge unchanged.
        assert!(merged.break_reminder.enabled);
        assert_eq!(merged.break_reminder.interval_minutes, 50);
        assert_eq!(merged.workout.time_of_day, "18:00");
        assert_eq!(merged.position.corner, ScreenCorner::TopRight);
        assert!(merged.autostart.enabled);
    }

    #[test]
    fn autostart_defaults_to_enabled() {
        assert!(Settings::default().autostart.enabled);
    }

    #[test]
    fn old_settings_json_missing_autostart_key_defaults_to_enabled() {
        // Same migration-safety guarantee as `position`: a settings.json
        // written before this field existed must still deserialize, with
        // autostart defaulting on rather than failing to load entirely.
        let mut base = serde_json::to_value(Settings::default()).unwrap();
        base.as_object_mut().unwrap().remove("autostart");

        let merged: Settings = serde_json::from_value(base).unwrap();
        assert!(merged.autostart.enabled);
    }

    #[test]
    fn merge_only_patches_the_named_nested_field() {
        // A patch touching just `workout.enabled` must not reset
        // `workout.timeOfDay` back to some default — merge_json must
        // recurse into nested objects rather than replacing them wholesale.
        let mut base = serde_json::to_value(Settings::default()).unwrap();
        let patch = json!({ "workout": { "enabled": false } });

        merge_json(&mut base, &patch);

        // Since `WorkoutSettings` requires both fields, deserialization
        // would fail if `timeOfDay` were dropped by the merge — so a
        // successful deserialize here is itself part of the assertion.
        let merged: Settings = serde_json::from_value(base).unwrap();
        assert!(!merged.workout.enabled);
        assert_eq!(merged.workout.time_of_day, "18:00");
    }

    #[test]
    fn merge_replaces_scalar_leaves_not_objects() {
        let mut base = json!({ "a": { "b": 1, "c": 2 } });
        let patch = json!({ "a": { "b": 99 } });

        merge_json(&mut base, &patch);

        assert_eq!(base, json!({ "a": { "b": 99, "c": 2 } }));
    }

    #[test]
    fn sanitize_clamps_zero_interval_up_to_minimum() {
        let mut settings = Settings::default();
        settings.water.interval_minutes = 0;
        settings.break_reminder.interval_minutes = 0;

        sanitize(&mut settings);

        assert_eq!(settings.water.interval_minutes, MIN_INTERVAL_MINUTES);
        assert_eq!(
            settings.break_reminder.interval_minutes,
            MIN_INTERVAL_MINUTES
        );
    }

    #[test]
    fn sanitize_clamps_absurdly_large_interval_down_to_maximum() {
        let mut settings = Settings::default();
        settings.water.interval_minutes = u32::MAX;

        sanitize(&mut settings);

        assert_eq!(settings.water.interval_minutes, MAX_INTERVAL_MINUTES);
    }

    #[test]
    fn sanitize_leaves_in_range_values_unchanged() {
        let mut settings = Settings::default();
        settings.water.interval_minutes = 45;

        sanitize(&mut settings);

        assert_eq!(settings.water.interval_minutes, 45);
    }

    #[test]
    fn sanitize_resets_malformed_time_of_day_to_default() {
        // A hand-edited settings.json with a broken timeOfDay must self-heal
        // on load rather than silently never firing forever.
        let mut settings = Settings::default();
        settings.workout.time_of_day = "not-a-time".into();

        sanitize(&mut settings);

        assert_eq!(
            settings.workout.time_of_day,
            Settings::default().workout.time_of_day
        );
    }

    #[test]
    fn sanitize_leaves_valid_time_of_day_unchanged() {
        let mut settings = Settings::default();
        settings.workout.time_of_day = "07:30".into();

        sanitize(&mut settings);

        assert_eq!(settings.workout.time_of_day, "07:30");
    }

    #[test]
    fn valid_time_of_day_formats() {
        assert!(is_valid_time_of_day("00:00"));
        assert!(is_valid_time_of_day("18:00"));
        assert!(is_valid_time_of_day("23:59"));
    }

    #[test]
    fn rejects_out_of_range_or_malformed_time_of_day() {
        assert!(!is_valid_time_of_day("24:00")); // hour out of range
        assert!(!is_valid_time_of_day("12:60")); // minute out of range
        assert!(!is_valid_time_of_day("9:00")); // missing leading zero
        assert!(!is_valid_time_of_day("18:0")); // missing leading zero
        assert!(!is_valid_time_of_day("18-00")); // wrong separator
        assert!(!is_valid_time_of_day("")); // empty
        assert!(!is_valid_time_of_day("garbage"));
    }

    #[test]
    fn update_settings_rejects_malformed_time_via_merge() {
        // update_settings can't be called directly here (it needs a live
        // AppHandle for the store), but the validation step it runs is
        // exercised the same way: merge onto defaults, then validate.
        let mut base = serde_json::to_value(Settings::default()).unwrap();
        let patch = json!({ "workout": { "timeOfDay": "not-a-time" } });
        merge_json(&mut base, &patch);
        let merged: Settings = serde_json::from_value(base).unwrap();

        assert!(!is_valid_time_of_day(&merged.workout.time_of_day));
    }
}
