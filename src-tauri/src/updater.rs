use crate::state::{self, UpdateInfo};
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

/// Holds the `Update` handle from the launch-time check, so `install_update`
/// can act on it directly instead of re-checking. Backend-only runtime
/// state — never serialized to the frontend, unlike `state::UpdateInfo`
/// (which is the summary the Renderer actually sees). Mirrors `voice.rs`'s
/// `TTS_ENGINE`/`MEM_CACHE` statics: module-owned runtime state rather than
/// something bolted onto `state::AppState`.
static PENDING_UPDATE: OnceLock<Mutex<Option<Update>>> = OnceLock::new();

/// Fire-and-forget: checks for a newer release once, on launch. Spawned as
/// a plain async task rather than run inline in `.setup()` — same
/// non-eager pattern as `voice::maybe_start_model_download`, deliberately
/// not the kind of eager, settle-racing work that crashed startup before
/// (this is just an HTTP GET, not an FFI model load).
pub fn maybe_check_for_update(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tracing::info!("checking for a Fairy update");
        let updater = match app.updater() {
            Ok(u) => u,
            Err(err) => {
                tracing::error!(error = %err, "updater() failed to initialize");
                return;
            }
        };
        let update = match updater.check().await {
            Ok(Some(update)) => update,
            Ok(None) => {
                tracing::info!("no update available");
                return;
            }
            Err(err) => {
                tracing::error!(error = %err, "update check failed");
                return;
            }
        };
        tracing::info!(version = %update.version, "update available");

        let info = UpdateInfo {
            version: update.version.clone(),
            notes: update.body.clone().unwrap_or_default(),
        };

        let cell = PENDING_UPDATE.get_or_init(|| Mutex::new(None));
        if let Ok(mut guard) = cell.lock() {
            *guard = Some(update);
        }

        state::set_update_available(&app, info);
    });
}

/// Downloads and installs the update found by `maybe_check_for_update`,
/// then relaunches into it. Errors (no pending update, download failure,
/// signature mismatch) surface to the Renderer as a string so it can show
/// a brief failure message and fall back to idle, rather than hanging on
/// "Updating…" forever.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    tracing::info!("install_update invoked");
    let update = {
        let cell = PENDING_UPDATE.get_or_init(|| Mutex::new(None));
        let mut guard = cell
            .lock()
            .map_err(|_| "update state poisoned".to_string())?;
        guard.take()
    }
    .ok_or_else(|| {
        tracing::error!("install_update: no update pending");
        "no update pending".to_string()
    })?;

    update
        .download_and_install(|_chunk_len, _total_len| {}, || {})
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "install_update: download_and_install failed");
            e.to_string()
        })?;

    tracing::info!("install_update: succeeded, restarting");
    // Diverges: exits and relaunches the process, never returning control.
    app.restart();
}
