use crate::settings::load_settings;
use crate::state::{self, Mode, ReminderType};
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
const REMINDER_DISPLAY: Duration = Duration::from_secs(5);
const IDLE_BARK_MIN_GAP: Duration = Duration::from_secs(45 * 60);

const IDLE_BARK_LINES: &[&str] = &[
    "Just checking in, master.",
    "Don't forget I'm here, master.",
    "It's quiet today, master.",
];

pub fn start_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let timers = Mutex::new(Timers::default());
        loop {
            tokio::time::sleep(TICK).await;
            tick(&app, &timers);
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

fn tick(app: &AppHandle, timers: &Mutex<Timers>) {
    if state::current_mode(app) != Mode::Idle {
        return;
    }

    let settings = load_settings(app);
    let mut timers = timers.lock().unwrap();
    let now = Instant::now();

    if settings.water.enabled
        && elapsed_at_least(
            now,
            timers.water_last,
            Duration::from_secs(settings.water.interval_minutes as u64 * 60),
        )
    {
        timers.water_last = now;
        fire(
            app,
            ReminderType::Water,
            "Time to drink some water, master.".into(),
        );
        return;
    }

    if settings.break_reminder.enabled
        && elapsed_at_least(
            now,
            timers.break_last,
            Duration::from_secs(settings.break_reminder.interval_minutes as u64 * 60),
        )
    {
        timers.break_last = now;
        fire(
            app,
            ReminderType::Break,
            "Stand up and stretch for 5, master.".into(),
        );
        return;
    }

    if settings.workout.enabled {
        let local_now = Local::now();
        if should_fire_workout(local_now, &settings.workout.time_of_day, timers.workout_last_date)
        {
            timers.workout_last_date = Some(local_now.date_naive());
            fire(app, ReminderType::Workout, "Workout time, master.".into());
            return;
        }
    }

    if settings.idle_bark.enabled && elapsed_at_least(now, timers.idle_bark_last, IDLE_BARK_MIN_GAP)
    {
        timers.idle_bark_last = now;
        let index = (chrono::Utc::now().timestamp() as usize) % IDLE_BARK_LINES.len();
        fire(app, ReminderType::IdleBark, IDLE_BARK_LINES[index].into());
    }
}

fn fire(app: &AppHandle, kind: ReminderType, message: String) {
    let triggered_at = chrono::Utc::now().timestamp_millis();
    state::set_reminder(app, kind, message, triggered_at);

    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(REMINDER_DISPLAY).await;
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
}
