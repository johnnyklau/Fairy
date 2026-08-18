use std::sync::OnceLock;
use tauri::{AppHandle, Manager};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{fmt, EnvFilter};

// tracing-appender's non-blocking writer flushes on a background thread and
// stops flushing the moment its WorkerGuard drops — which would happen
// immediately if init() just let it fall out of scope at the end of the
// function. Parking it here keeps it alive for the process's whole
// lifetime, matching main()'s implicit "hold the guard until exit" pattern
// from tracing-appender's own docs, adapted for a callback-shaped .setup().
static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

/// Initializes logging as the first step in `.setup()`, so as much of
/// startup as possible is covered. Always writes at `info` and above to a
/// daily-rotating file under the app's own data directory — the same place
/// settings.json and the voice model already live — so there's always
/// something to look at after the fact, not just when a shell happened to
/// have `RUST_LOG` set. Set `RUST_LOG` (e.g. `RUST_LOG=fairy_lib=debug`)
/// to see finer-grained detail (cache hits, scheduler ticks that decided
/// not to fire, etc.) without needing a separate debug build — see
/// docs/LOGGING.md.
pub fn init(app: &AppHandle) {
    let log_dir = app
        .path()
        .app_data_dir()
        .expect("app_data_dir should resolve on desktop platforms")
        .join("logs");

    let file_appender = tracing_appender::rolling::daily(&log_dir, "fairy.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let _ = LOG_GUARD.set(guard);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = tracing_subscriber::registry().with(filter).with(
        fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false)
            .with_target(true),
    );

    if tracing::subscriber::set_global_default(subscriber).is_err() {
        // Already initialized (shouldn't happen — this is called once from
        // .setup() — but a second call silently keeping the first
        // subscriber is safer than panicking over a logging setup issue).
        eprintln!("logging::init called more than once; ignoring");
    }
}
