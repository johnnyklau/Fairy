use crate::dialogues::{self, Line};
use crate::settings::load_settings;
use crate::state::{self, Mode, ReminderType, Settings};
use crate::voice;
use chrono::Local;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::AppHandle;

struct Timers {
    water_last: Instant,
    break_last: Instant,
    workout_last_date: Option<chrono::NaiveDate>,
    idle_bark_last: Instant,
}

impl Default for Timers {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            water_last: now,
            break_last: now,
            workout_last_date: None,
            idle_bark_last: now,
        }
    }
}

const TICK: Duration = Duration::from_secs(15);
const REMINDER_DISPLAY: Duration = Duration::from_secs(10);
const IDLE_BARK_MIN_GAP: Duration = Duration::from_secs(45 * 60);
const AUDIO_TAIL_BUFFER: Duration = Duration::from_millis(500);

pub fn start_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let timers = Mutex::new(Timers::default());
        loop {
            tokio::time::sleep(TICK).await;
            tick(&app, &timers).await;
        }
    });
}

/// Has at least `threshold` elapsed since `last`? The core decision behind
/// water/break intervals and the idle-bark gap — isolated from `Settings`/
/// `AppHandle` so it's testable without waiting on real time.
fn elapsed_at_least(now: Instant, last: Instant, threshold: Duration) -> bool {
    now.duration_since(last) >= threshold
}

/// Should the workout reminder fire right now? True only when the current
/// local time matches `time_of_day` exactly (HH:MM) and it hasn't already
/// fired today — otherwise it would refire on every tick for the whole
/// matching minute.
fn should_fire_workout(
    now: chrono::DateTime<chrono::Local>,
    time_of_day: &str,
    last_fired_date: Option<chrono::NaiveDate>,
) -> bool {
    let matches_time = now.format("%H:%M").to_string() == time_of_day;
    let already_fired_today = last_fired_date == Some(now.date_naive());
    matches_time && !already_fired_today
}

/// The final display duration for a reminder popup: at least the fixed
/// `REMINDER_DISPLAY` window, or long enough to cover the attached audio
/// clip plus a trailing buffer, whichever is longer — so voice, once
/// enabled, never gets cut off mid-sentence. `None` (voice off, synthesis
/// failed, or timed out) always yields `fixed`, unchanged from pre-voice
/// behavior.
fn dismiss_duration(fixed: Duration, clip_duration_ms: Option<u32>) -> Duration {
    match clip_duration_ms {
        Some(ms) => fixed.max(Duration::from_millis(ms as u64) + AUDIO_TAIL_BUFFER),
        None => fixed,
    }
}

enum Decision {
    Reminder {
        kind: ReminderType,
        line: &'static Line,
    },
    IdleBark {
        line: &'static Line,
    },
}

/// Which single reminder (if any) should fire this tick, and updates the
/// relevant timer. Kept synchronous and separate from `fire()` so the
/// `Timers` lock is always released before any `.await` point — a
/// `MutexGuard` held across an await is both a footgun and, for the
/// std `Mutex` used here, not `Send`-safe to hold that way.
fn decide(settings: &Settings, timers: &Mutex<Timers>, now: Instant) -> Option<Decision> {
    let mut timers = timers.lock().unwrap();

    if settings.water.enabled
        && elapsed_at_least(
            now,
            timers.water_last,
            Duration::from_secs(settings.water.interval_minutes as u64 * 60),
        )
    {
        timers.water_last = now;
        return Some(Decision::Reminder {
            kind: ReminderType::Water,
            line: dialogues::pick(dialogues::WATER_LINES),
        });
    }

    if settings.break_reminder.enabled
        && elapsed_at_least(
            now,
            timers.break_last,
            Duration::from_secs(settings.break_reminder.interval_minutes as u64 * 60),
        )
    {
        timers.break_last = now;
        return Some(Decision::Reminder {
            kind: ReminderType::Break,
            line: dialogues::pick(dialogues::BREAK_LINES),
        });
    }

    if settings.workout.enabled {
        let local_now = Local::now();
        if should_fire_workout(
            local_now,
            &settings.workout.time_of_day,
            timers.workout_last_date,
        ) {
            timers.workout_last_date = Some(local_now.date_naive());
            return Some(Decision::Reminder {
                kind: ReminderType::Workout,
                line: dialogues::pick(dialogues::WORKOUT_LINES),
            });
        }
    }

    if settings.idle_bark.enabled && elapsed_at_least(now, timers.idle_bark_last, IDLE_BARK_MIN_GAP)
    {
        timers.idle_bark_last = now;
        return Some(Decision::IdleBark {
            line: dialogues::pick(dialogues::IDLE_BARK_LINES),
        });
    }

    None
}

async fn tick(app: &AppHandle, timers: &Mutex<Timers>) {
    if state::current_mode(app) != Mode::Idle {
        return;
    }

    let settings = load_settings(app);
    let now = Instant::now();

    match decide(&settings, timers, now) {
        Some(Decision::Reminder { kind, line }) => {
            show_reminder(app, kind, line, &settings).await;
        }
        Some(Decision::IdleBark { line }) => {
            show_reminder(app, ReminderType::IdleBark, line, &settings).await;
        }
        None => {}
    }
}

/// Synthesizes (if voice is enabled), attaches audio to the reminder, and
/// schedules its dismissal. `line.en` is always what's displayed;
/// `line.for_language(settings.voice.language)` is what gets spoken — see
/// VOICE_SPEC.md "Scope decisions": display never localizes, only audio
/// does. Pairing both in one `Line` means there's no separate index to
/// keep in sync between the two.
async fn show_reminder(
    app: &AppHandle,
    kind: ReminderType,
    line: &'static Line,
    settings: &Settings,
) {
    let triggered_at = chrono::Utc::now().timestamp_millis();

    let audio = if settings.voice.enabled {
        voice::synthesize(
            app,
            line.for_language(settings.voice.language),
            settings.voice.language,
            settings.voice.volume,
        )
        .await
    } else {
        None
    };

    let dismiss_after = dismiss_duration(REMINDER_DISPLAY, audio.as_ref().map(|a| a.duration_ms));

    state::set_reminder(app, kind, line.en.to_string(), triggered_at, audio);

    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(dismiss_after).await;
        if state::current_mode(&app_clone) == Mode::Reminder {
            state::set_mode_idle(&app_clone);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn elapsed_at_least_false_before_threshold() {
        let now = Instant::now();
        let last = now - Duration::from_secs(30);
        assert!(!elapsed_at_least(now, last, Duration::from_secs(60)));
    }

    #[test]
    fn elapsed_at_least_true_after_threshold() {
        let now = Instant::now();
        let last = now - Duration::from_secs(120);
        assert!(elapsed_at_least(now, last, Duration::from_secs(60)));
    }

    #[test]
    fn elapsed_at_least_true_at_exact_boundary() {
        let now = Instant::now();
        let last = now - Duration::from_secs(60);
        assert!(elapsed_at_least(now, last, Duration::from_secs(60)));
    }

    fn local_dt(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> chrono::DateTime<chrono::Local> {
        chrono::Local.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn workout_fires_when_time_matches_and_not_yet_fired_today() {
        let now = local_dt(2026, 8, 6, 18, 0);
        assert!(should_fire_workout(now, "18:00", None));
    }

    #[test]
    fn workout_does_not_fire_when_time_does_not_match() {
        let now = local_dt(2026, 8, 6, 18, 1);
        assert!(!should_fire_workout(now, "18:00", None));
    }

    #[test]
    fn workout_does_not_refire_same_day() {
        let now = local_dt(2026, 8, 6, 18, 0);
        let already_fired = now.date_naive();
        assert!(!should_fire_workout(now, "18:00", Some(already_fired)));
    }

    #[test]
    fn workout_fires_again_on_a_new_day() {
        let yesterday = local_dt(2026, 8, 5, 18, 0).date_naive();
        let now = local_dt(2026, 8, 6, 18, 0);
        assert!(should_fire_workout(now, "18:00", Some(yesterday)));
    }

    #[test]
    fn dismiss_duration_uses_fixed_window_when_no_audio() {
        assert_eq!(dismiss_duration(REMINDER_DISPLAY, None), REMINDER_DISPLAY);
    }

    #[test]
    fn dismiss_duration_uses_fixed_window_when_clip_is_shorter() {
        // A 1s clip + 500ms buffer (1.5s) is still shorter than the fixed
        // window, so the fixed window wins.
        assert_eq!(
            dismiss_duration(REMINDER_DISPLAY, Some(1000)),
            REMINDER_DISPLAY
        );
    }

    #[test]
    fn dismiss_duration_extends_past_fixed_window_for_long_clips() {
        // A clip long enough that clip + 500ms buffer exceeds the fixed
        // window, regardless of REMINDER_DISPLAY's current value.
        let clip_ms = REMINDER_DISPLAY.as_millis() as u32 + 2000;
        let result = dismiss_duration(REMINDER_DISPLAY, Some(clip_ms));
        assert_eq!(result, Duration::from_millis((clip_ms + 500) as u64));
    }

    #[test]
    fn dismiss_duration_at_exact_boundary_prefers_fixed() {
        // Clip + buffer exactly equal to the fixed window: `max` picks
        // either (they're equal), asserting on the value not the branch.
        let clip_plus_buffer_eq_fixed = REMINDER_DISPLAY - AUDIO_TAIL_BUFFER;
        let result = dismiss_duration(
            REMINDER_DISPLAY,
            Some(clip_plus_buffer_eq_fixed.as_millis() as u32),
        );
        assert_eq!(result, REMINDER_DISPLAY);
    }
}
