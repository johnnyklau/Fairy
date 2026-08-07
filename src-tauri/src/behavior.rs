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

fn tick(app: &AppHandle, timers: &Mutex<Timers>) {
    if state::current_mode(app) != Mode::Idle {
        return;
    }

    let settings = load_settings(app);
    let mut timers = timers.lock().unwrap();
    let now = Instant::now();

    if settings.water.enabled
        && now.duration_since(timers.water_last)
            >= Duration::from_secs(settings.water.interval_minutes as u64 * 60)
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
        && now.duration_since(timers.break_last)
            >= Duration::from_secs(settings.break_reminder.interval_minutes as u64 * 60)
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
        let today = local_now.date_naive();
        let matches_time = local_now.format("%H:%M").to_string() == settings.workout.time_of_day;
        let already_fired_today = timers.workout_last_date == Some(today);
        if matches_time && !already_fired_today {
            timers.workout_last_date = Some(today);
            fire(app, ReminderType::Workout, "Workout time, master.".into());
            return;
        }
    }

    if settings.idle_bark.enabled
        && now.duration_since(timers.idle_bark_last) >= IDLE_BARK_MIN_GAP
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
