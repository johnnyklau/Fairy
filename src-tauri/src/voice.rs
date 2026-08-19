use crate::dialogues;
use crate::state::{ReminderAudio, VoiceLanguage, VoiceSettings};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsSupertonicModelConfig,
};
use std::collections::HashMap;
use std::f32::consts::PI;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const SPEAKER_ID: i32 = 0;
// <1.0 slower, >1.0 faster; the model's natural pace (1.0) read as too
// fast for a wellness reminder meant to actually be heard and absorbed.
const SPEAKING_SPEED: f32 = 0.8;
pub const SYNTHESIS_TIMEOUT: Duration = Duration::from_secs(3);
// Bounds the *entire* model download (DNS, connect, and reading the ~145MB
// body), not just connecting — ureq's per-request `.timeout()` has no
// default, so a stalled connection would otherwise hang this background
// task forever with the Settings UI's progress indicator stuck mid-percent.
// Generous on purpose: this is a one-time, large download that should
// still succeed on a slow connection, just not hang indefinitely on a dead
// one.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const PEAK_CEILING: f32 = 0.98;

const MODEL_DIR_NAME: &str = "sherpa-onnx-supertonic-3-tts-int8-2026-05-11";
// Bumped whenever a synthesis parameter this file controls — speed, pitch,
// anything in the effects chain — changes. Folded into the cache key
// alongside MODEL_DIR_NAME (see cache_key's own doc comment) so a tuning
// change can't silently keep serving audio generated under the old
// settings; the alternative would be remembering to clear voice_cache by
// hand every time one of these gets adjusted.
const SYNTHESIS_PARAMS_VERSION: &str = "2";
const MODEL_ARCHIVE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/sherpa-onnx-supertonic-3-tts-int8-2026-05-11.tar.bz2";
// Sourced from GitHub's release API `digest` field for this asset
// (`GET /repos/k2-fsa/sherpa-onnx/releases/tags/tts-models`), computed by
// GitHub on upload — not a third-party claim. Re-check this if
// MODEL_ARCHIVE_URL is ever bumped to a newer dated release.
const MODEL_ARCHIVE_SHA256: &str =
    "82fa96f91c4ef8abaae3a14a3f4153facf88bed821d1f7331cec2700f432c427";

const MODEL_FILES: &[&str] = &[
    "duration_predictor.int8.onnx",
    "text_encoder.int8.onnx",
    "vector_estimator.int8.onnx",
    "vocoder.int8.onnx",
    "tts.json",
    "unicode_indexer.bin",
    "voice.bin",
];

// Records each model file's byte size at the time it was last verified as
// correctly extracted — cheap enough (a handful of stat() calls) to check
// on every `is_model_ready`, unlike re-hashing ~145MB. Exists because a
// single model file was found silently truncated on disk weeks after a
// successful download (cause never fully pinned down — a conflicting
// concurrent extraction is the leading theory), and nothing detected it:
// `is_file()` alone can't tell a truncated file from a whole one, so the
// corruption sat undetected until the disk cache that was masking it
// happened to run dry. Named with a leading dot so it doesn't collide with
// anything sherpa-onnx's own archive might ever ship.
const MODEL_MANIFEST_FILE: &str = ".manifest.json";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    downloaded_bytes: u64,
    total_bytes: u64,
}

fn lang_code(language: VoiceLanguage) -> &'static str {
    match language {
        VoiceLanguage::En => "en",
        VoiceLanguage::Ja => "ja",
    }
}

// ---------- Paths ----------

fn app_data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .expect("app_data_dir should resolve on desktop platforms")
}

fn model_dir(app: &AppHandle) -> PathBuf {
    app_data_dir(app).join("voice_model").join(MODEL_DIR_NAME)
}

fn cache_dir(app: &AppHandle) -> PathBuf {
    app_data_dir(app).join("voice_cache")
}

/// True only if every model file exists *and* its size matches what was
/// recorded the last time this directory was verified as a correct
/// extraction — catches a truncated/corrupted file that `.is_file()` alone
/// would miss, cheaply enough to call before every synthesis attempt.
///
/// If no manifest exists yet (installs made before this check existed),
/// one is generated from whatever's on disk right now rather than forcing
/// a ~145MB re-download for everyone upgrading — this only protects
/// against corruption *from this point forward*. A fresh download always
/// gets a manifest written from the just-verified extraction (see
/// `download_and_install_model_blocking`), so this lazy path is purely a
/// one-time backward-compatibility step for pre-existing installs.
pub fn is_model_ready(app: &AppHandle) -> bool {
    model_dir_ready(&model_dir(app))
}

fn model_dir_ready(dir: &Path) -> bool {
    if !MODEL_FILES.iter().all(|f| dir.join(f).is_file()) {
        tracing::debug!(dir = %dir.display(), "model not ready: a file is missing");
        return false;
    }
    let manifest = match read_model_manifest(dir) {
        Some(m) => m,
        None => {
            tracing::warn!(
                dir = %dir.display(),
                "no readable model manifest — generating one from files on disk (backward-compat self-heal, not a fresh verified download)"
            );
            match write_model_manifest(dir) {
                Some(m) => m,
                None => return false,
            }
        }
    };
    let ready = MODEL_FILES.iter().all(|f| {
        let matches = fs::metadata(dir.join(f))
            .map(|meta| manifest.get(*f) == Some(&meta.len()))
            .unwrap_or(false);
        if !matches {
            tracing::warn!(
                file = f,
                "model file size doesn't match manifest — treating model as corrupted"
            );
        }
        matches
    });
    tracing::debug!(ready, "model_dir_ready check complete");
    ready
}

fn read_model_manifest(dir: &Path) -> Option<HashMap<String, u64>> {
    let contents = fs::read_to_string(dir.join(MODEL_MANIFEST_FILE)).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Records the current on-disk size of every model file. Called both as
/// the one-time backward-compat path above, and — the path that actually
/// matters for new installs — right after a fresh download's extraction,
/// so the recorded sizes always trace back to a checksum-verified archive.
fn write_model_manifest(dir: &Path) -> Option<HashMap<String, u64>> {
    let mut manifest = HashMap::new();
    for file in MODEL_FILES {
        let size = fs::metadata(dir.join(file)).ok()?.len();
        manifest.insert((*file).to_string(), size);
    }
    let json = serde_json::to_string(&manifest).ok()?;
    fs::write(dir.join(MODEL_MANIFEST_FILE), json).ok()?;
    Some(manifest)
}

// ---------- Model provisioning ----------

/// Guards against two overlapping downloads. `maybe_start_model_download`
/// is called from `settings::update_settings` on *every* settings change,
/// not just voice ones — without this, changing anything else (position,
/// autostart, volume) while a download was already in flight would spawn a
/// second `tar::Archive::unpack` into the same directory, since
/// `is_model_ready` alone stays false for the whole download and can't
/// tell "not ready yet" apart from "not ready, already being fixed".
/// Concurrent, uncoordinated extractions into the same files is exactly
/// the leading theory (see `SYNTHESIS_PARAMS_VERSION`'s sibling comment on
/// `MODEL_MANIFEST_FILE`) for a real corruption incident this project
/// already hit once.
static MODEL_DOWNLOAD_IN_FLIGHT: OnceLock<Mutex<bool>> = OnceLock::new();

/// Fire-and-forget: kicks off a background download if voice was just
/// enabled and the model isn't present yet. Mirrors the
/// `apply_window_position`/`apply_autostart` side-effect pattern already
/// used from `settings::update_settings`.
pub fn maybe_start_model_download(app: &AppHandle, voice: &VoiceSettings) {
    if !voice.enabled || is_model_ready(app) {
        return;
    }

    let flag = MODEL_DOWNLOAD_IN_FLIGHT.get_or_init(|| Mutex::new(false));
    {
        let Ok(mut in_flight) = flag.lock() else {
            return;
        };
        if *in_flight {
            tracing::debug!("voice model download already in flight, not starting another");
            return;
        }
        *in_flight = true;
    }

    tracing::info!("voice model not ready, starting background download");
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match download_and_install_model(&app).await {
            Ok(()) => tracing::info!("voice model download completed"),
            Err(err) => tracing::error!(error = %err, "voice model download failed"),
        }
        if let Ok(mut in_flight) = MODEL_DOWNLOAD_IN_FLIGHT
            .get_or_init(|| Mutex::new(false))
            .lock()
        {
            *in_flight = false;
        }
    });
}

/// Fire-and-forget: if the model is already on disk, warm it into memory
/// now rather than paying that cold-load cost on the first real reminder.
/// A no-op if the model isn't present yet (that path is instead handled by
/// `maybe_start_model_download`, and the first synthesis call after a
/// download completes just lazy-loads normally).
pub fn maybe_eager_load(app: &AppHandle, voice: &VoiceSettings) {
    if !voice.enabled || !is_model_ready(app) || tts_already_loaded() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Model loading crosses into sherpa-onnx's FFI boundary — wrapped
        // so a panic there degrades to "voice stays unavailable" instead
        // of taking the whole app down at every future launch (this runs
        // unconditionally in .setup() whenever voice.enabled is true).
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ensure_tts_loaded(&app);
        }));
    });
}

async fn download_and_install_model(app: &AppHandle) -> Result<(), String> {
    let app_for_blocking = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        download_and_install_model_blocking(&app_for_blocking)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn download_and_install_model_blocking(app: &AppHandle) -> Result<(), String> {
    let archive_bytes = download_with_progress(app, MODEL_ARCHIVE_URL)?;
    if !MODEL_ARCHIVE_SHA256.is_empty() {
        verify_checksum(&archive_bytes, MODEL_ARCHIVE_SHA256)?;
    }
    extract_archive(app, &archive_bytes)?;
    // Written from the extraction that was just verified via the archive
    // checksum above — every later `is_model_ready` check traces back to
    // this known-good moment, not just "a file happened to exist".
    write_model_manifest(&model_dir(app)).ok_or("failed to record model manifest")?;
    Ok(())
}

fn download_with_progress(app: &AppHandle, url: &str) -> Result<Vec<u8>, String> {
    let response = ureq::get(url)
        .timeout(DOWNLOAD_TIMEOUT)
        .call()
        .map_err(|e| e.to_string())?;
    let total_bytes: u64 = response
        .header("Content-Length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut reader = response.into_reader();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    loop {
        let n = reader.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        downloaded += n as u64;
        let _ = app.emit(
            "voice_model_download_progress",
            DownloadProgress {
                downloaded_bytes: downloaded,
                total_bytes,
            },
        );
    }
    Ok(buf)
}

fn verify_checksum(bytes: &[u8], expected_hex: &str) -> Result<(), String> {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let actual = to_hex(&hasher.finalize());
    if actual.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(format!(
            "model archive checksum mismatch: expected {expected_hex}, got {actual}"
        ))
    }
}

fn extract_archive(app: &AppHandle, archive_bytes: &[u8]) -> Result<(), String> {
    let decompressed = bzip2::read::BzDecoder::new(archive_bytes);
    let mut archive = tar::Archive::new(decompressed);
    let dest = app_data_dir(app).join("voice_model");
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    archive.unpack(&dest).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- Synthesis ----------

static TTS_ENGINE: OnceLock<Mutex<Option<OfflineTts>>> = OnceLock::new();
static MEM_CACHE: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();

/// Includes `model_id` (in practice always `MODEL_DIR_NAME` joined with
/// `SYNTHESIS_PARAMS_VERSION`) so a future model swap *or* a tuning change
/// to speed/pitch/the effects chain can't silently keep serving audio
/// generated under the old settings — bumping either half changes every
/// key, making the old on-disk/in-memory entries unreachable dead weight
/// rather than wrongly-reused hits. Taken as a parameter (not read from the
/// constants directly) so this is unit-testable without depending on the
/// real model name.
fn cache_key(model_id: &str, text: &str, language: VoiceLanguage) -> String {
    let mut hasher = Sha256::new();
    hasher.update(model_id.as_bytes());
    hasher.update(b":");
    hasher.update(lang_code(language).as_bytes());
    hasher.update(b":");
    hasher.update(text.as_bytes());
    to_hex(&hasher.finalize())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Synthesize `text` (already resolved to the target language — see the
/// `*_voice_text` lookups above) as processed WAV audio, or `None` if the
/// model isn't ready, synthesis fails, or it exceeds `SYNTHESIS_TIMEOUT`.
/// Behavior is responsible for falling back to a silent reminder on `None`.
/// `volume` (0.0-1.0, from `settings.voice.volume`) is applied fresh on
/// every call, after the cache lookup/synthesis — the cache always stores
/// the canonical (unscaled) processed clip, so moving the volume slider
/// never invalidates it or forces a re-synthesis.
pub async fn synthesize(
    app: &AppHandle,
    text: &str,
    language: VoiceLanguage,
    volume: f32,
) -> Option<ReminderAudio> {
    let start = std::time::Instant::now();
    if text.is_empty() {
        tracing::debug!("synthesize: empty text, skipping");
        return None;
    }
    if !is_model_ready(app) {
        tracing::warn!(text, "synthesize: model not ready, no audio will play");
        return None;
    }

    let key = cache_key(
        &format!("{MODEL_DIR_NAME}:{SYNTHESIS_PARAMS_VERSION}"),
        text,
        language,
    );
    let canonical_bytes = if let Some(bytes) = mem_cache_get(&key) {
        tracing::debug!(text, "synthesize: memory cache hit");
        bytes
    } else if let Some(bytes) = disk_cache_get(app, &key) {
        tracing::debug!(text, "synthesize: disk cache hit");
        mem_cache_put(key.clone(), bytes.clone());
        bytes
    } else {
        tracing::info!(text, language = ?language, "synthesize: cache miss, running inference");
        let app_for_blocking = app.clone();
        let text_owned = text.to_string();
        let handle = tauri::async_runtime::spawn_blocking(move || {
            // See the eager-load call site for why this is wrapped —
            // same FFI boundary, same "degrade to no audio" contract.
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                synthesize_blocking(&app_for_blocking, &text_owned, language)
            }))
            .unwrap_or(None)
        });

        let bytes = match tokio::time::timeout(SYNTHESIS_TIMEOUT, handle).await {
            Ok(Ok(Some(bytes))) => bytes,
            Ok(Ok(None)) => {
                tracing::error!(text, "synthesize: inference returned no audio");
                return None;
            }
            Ok(Err(join_err)) => {
                tracing::error!(text, error = %join_err, "synthesize: blocking task panicked/was cancelled");
                return None;
            }
            Err(_) => {
                tracing::error!(text, timeout = ?SYNTHESIS_TIMEOUT, "synthesize: timed out");
                return None;
            }
        };

        disk_cache_put(app, &key, &bytes);
        mem_cache_put(key, bytes.clone());
        bytes
    };

    let scaled = apply_volume(&canonical_bytes, volume);
    let duration_ms = read_wav_duration_ms(&scaled).unwrap_or(0);
    tracing::info!(
        text,
        duration_ms,
        elapsed_ms = start.elapsed().as_millis() as u64,
        "synthesize: done"
    );
    Some(ReminderAudio {
        base64_wav: BASE64.encode(&scaled),
        duration_ms,
    })
}

fn mem_cache_get(key: &str) -> Option<Vec<u8>> {
    let cache = MEM_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    cache.lock().ok()?.get(key).cloned()
}

fn mem_cache_put(key: String, wav_bytes: Vec<u8>) {
    let cache = MEM_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = cache.lock() {
        guard.insert(key, wav_bytes);
    }
}

fn disk_cache_get(app: &AppHandle, key: &str) -> Option<Vec<u8>> {
    let path = cache_dir(app).join(format!("{key}.wav"));
    fs::read(path).ok()
}

fn disk_cache_put(app: &AppHandle, key: &str, wav_bytes: &[u8]) {
    let dir = cache_dir(app);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = fs::write(dir.join(format!("{key}.wav")), wav_bytes);
}

fn build_tts_config(dir: &Path) -> OfflineTtsConfig {
    let path = |name: &str| Some(dir.join(name).to_string_lossy().into_owned());
    OfflineTtsConfig {
        model: sherpa_onnx::OfflineTtsModelConfig {
            supertonic: OfflineTtsSupertonicModelConfig {
                duration_predictor: path("duration_predictor.int8.onnx"),
                text_encoder: path("text_encoder.int8.onnx"),
                vector_estimator: path("vector_estimator.int8.onnx"),
                vocoder: path("vocoder.int8.onnx"),
                tts_json: path("tts.json"),
                unicode_indexer: path("unicode_indexer.bin"),
                voice_style: path("voice.bin"),
            },
            num_threads: 2,
            debug: false,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Cheap pre-check so `maybe_eager_load` — called from `update_settings` on
/// *every* settings change, not just voice ones — doesn't spawn a blocking
/// task just to have `ensure_tts_loaded` immediately no-op. A lock-and-check
/// is a fraction of the cost of a thread-pool spawn.
fn tts_already_loaded() -> bool {
    TTS_ENGINE
        .get()
        .is_some_and(|cell| cell.lock().map(|guard| guard.is_some()).unwrap_or(false))
}

fn ensure_tts_loaded(app: &AppHandle) -> bool {
    let cell = TTS_ENGINE.get_or_init(|| Mutex::new(None));
    let Ok(mut guard) = cell.lock() else {
        return false;
    };
    if guard.is_some() {
        return true;
    }
    let config = build_tts_config(&model_dir(app));
    match OfflineTts::create(&config) {
        Some(tts) => {
            *guard = Some(tts);
            true
        }
        None => false,
    }
}

fn synthesize_blocking(app: &AppHandle, text: &str, language: VoiceLanguage) -> Option<Vec<u8>> {
    if !ensure_tts_loaded(app) {
        return None;
    }
    let cell = TTS_ENGINE.get()?;
    let guard = cell.lock().ok()?;
    let tts = guard.as_ref()?;

    let mut extra = HashMap::new();
    extra.insert("lang".to_string(), serde_json::json!(lang_code(language)));
    let gen_config = GenerationConfig {
        sid: SPEAKER_ID,
        num_steps: 8,
        speed: SPEAKING_SPEED,
        extra: Some(extra),
        ..Default::default()
    };

    let audio = tts.generate_with_config(text, &gen_config, None::<fn(&[f32], f32) -> bool>)?;
    let sample_rate = audio.sample_rate() as f32;
    let mut samples: Vec<f32> = audio.samples().to_vec();

    samples = apply_pitch_shift(&samples, sample_rate, 3.0);
    apply_distortion(&mut samples, 0.2);
    apply_bitcrush(&mut samples, 11, 0.24);
    samples = apply_vibrato(&samples, sample_rate, 0.18, 1.0, 0.10);
    samples = apply_reverb(&samples, sample_rate, 0.2, 0.19);
    apply_eq(&mut samples, sample_rate);
    limit_peaks(&mut samples, PEAK_CEILING);

    Some(write_wav_bytes(&samples, sample_rate as u32))
}

// ---------- Effects chain (ported from the standalone voice-spike, spike-
// confirmed by ear on both English and Japanese reminder lines) ----------

fn apply_pitch_shift(samples: &[f32], sample_rate: f32, semitones: f32) -> Vec<f32> {
    use pitch_shift::{Shifter, TOTAL_F32};
    type State = Box<[f32; TOTAL_F32]>;
    let state_vec = vec![0.0; TOTAL_F32];
    let state_box: State = state_vec.try_into().unwrap();
    let mut shifter = Shifter::new(state_box);

    let mut padded = samples.to_vec();
    let rem = padded.len() % 128;
    if rem != 0 {
        padded.extend(std::iter::repeat_n(0.0, 128 - rem));
    }

    let mut out = Vec::with_capacity(padded.len());
    for chunk in padded.chunks_exact(128) {
        let out_chunk = shifter.shift(chunk, semitones, 128, sample_rate);
        out.extend_from_slice(out_chunk);
    }
    out.truncate(samples.len());
    out
}

fn apply_distortion(samples: &mut [f32], amount: f32) {
    let drive = 1.0 + amount * 15.0;
    for s in samples.iter_mut() {
        *s = (*s * drive).tanh();
    }
}

fn apply_bitcrush(samples: &mut [f32], bits: u32, mix: f32) {
    let levels = (1u32 << bits) as f32;
    for s in samples.iter_mut() {
        let crushed = (*s * levels).round() / levels;
        *s = *s * (1.0 - mix) + crushed * mix;
    }
}

fn apply_vibrato(
    samples: &[f32],
    sample_rate: f32,
    depth: f32,
    rate_hz: f32,
    mix: f32,
) -> Vec<f32> {
    let base_delay_samples = 5.0 * sample_rate / 1000.0;
    let depth_samples = depth * 5.0 * sample_rate / 1000.0;
    let mut out = Vec::with_capacity(samples.len());
    for (i, &dry) in samples.iter().enumerate() {
        let t = i as f32 / sample_rate;
        let lfo = (2.0 * PI * rate_hz * t).sin();
        let delay = base_delay_samples + depth_samples * lfo;
        let read_pos = i as f32 - delay;
        let wet = if read_pos >= 0.0 {
            let idx0 = read_pos.floor() as usize;
            let frac = read_pos - read_pos.floor();
            let s0 = samples.get(idx0).copied().unwrap_or(0.0);
            let s1 = samples.get(idx0 + 1).copied().unwrap_or(s0);
            s0 + (s1 - s0) * frac
        } else {
            0.0
        };
        out.push(dry * (1.0 - mix) + wet * mix);
    }
    out
}

fn apply_reverb(samples: &[f32], sample_rate: f32, decay_s: f32, mix: f32) -> Vec<f32> {
    let comb_delays_ms = [29.7f32, 37.1, 41.1, 43.7];
    let allpass_delays_ms = [5.0f32, 1.7];
    let feedback = 10f32.powf(-3.0 * (comb_delays_ms[0] / 1000.0) / decay_s.max(0.01));

    let mut wet = vec![0.0f32; samples.len()];
    for &delay_ms in &comb_delays_ms {
        let delay_samples = ((delay_ms / 1000.0) * sample_rate) as usize;
        if delay_samples == 0 {
            continue;
        }
        let mut buf = vec![0.0f32; delay_samples];
        let mut idx = 0;
        for (i, &s) in samples.iter().enumerate() {
            let delayed = buf[idx];
            let val = s + delayed * feedback;
            buf[idx] = val;
            wet[i] += val * 0.25;
            idx = (idx + 1) % delay_samples;
        }
    }
    for &delay_ms in &allpass_delays_ms {
        let delay_samples = ((delay_ms / 1000.0) * sample_rate) as usize;
        if delay_samples == 0 {
            continue;
        }
        let g = 0.5;
        let mut buf = vec![0.0f32; delay_samples];
        let mut idx = 0;
        for w in wet.iter_mut() {
            let delayed = buf[idx];
            let input = *w;
            let output = -g * input + delayed;
            buf[idx] = input + g * output;
            *w = output;
            idx = (idx + 1) % delay_samples;
        }
    }

    samples
        .iter()
        .zip(wet.iter())
        .map(|(&d, &w)| d * (1.0 - mix) + w * mix)
        .collect()
}

/// RBJ Audio EQ Cookbook biquad.
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    fn low_shelf(sample_rate: f32, freq: f32, gain_db: f32, slope: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let alpha = w0.sin() / 2.0 * ((a + 1.0 / a) * (1.0 / slope - 1.0) + 2.0).sqrt();
        let cos_w0 = w0.cos();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn high_shelf(sample_rate: f32, freq: f32, gain_db: f32, slope: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let alpha = w0.sin() / 2.0 * ((a + 1.0 / a) * (1.0 / slope - 1.0) + 2.0).sqrt();
        let cos_w0 = w0.cos();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn peaking(sample_rate: f32, freq: f32, gain_db: f32, q: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, x0: f32) -> f32 {
        let y0 = self.b0 * x0 + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x0;
        self.y2 = self.y1;
        self.y1 = y0;
        y0
    }
}

fn apply_eq(samples: &mut [f32], sample_rate: f32) {
    let mut low = Biquad::low_shelf(sample_rate, 200.0, -3.0, 1.0);
    let mut mid = Biquad::peaking(sample_rate, 1000.0, 4.0, 1.0);
    let mut high = Biquad::high_shelf(sample_rate, 4000.0, -10.0, 1.0);
    for s in samples.iter_mut() {
        let v = low.process(*s);
        let v = mid.process(v);
        let v = high.process(v);
        *s = v;
    }
}

/// Peak-normalize down to `ceiling` if the chain pushed the signal above it
/// (spiking found this happens at some parameter combinations, e.g. pitch
/// +3.5st + distortion 0.2 hit 1.045) — prevents the hard clipping that
/// `write_wav_bytes`'s clamp would otherwise silently introduce.
fn limit_peaks(samples: &mut [f32], ceiling: f32) {
    let max_abs = samples.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
    if max_abs > ceiling {
        let scale = ceiling / max_abs;
        for s in samples.iter_mut() {
            *s *= scale;
        }
    }
}

// ---------- WAV encoding (hand-rolled per VOICE_SPEC.md: "minimal WAV
// header", no external audio-encoding dependency) ----------

fn write_wav_bytes(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let byte_rate = sample_rate * 2;
    let block_align: u16 = 2;
    let data_len = (samples.len() as u32) * 2;
    let mut buf = Vec::with_capacity(44 + data_len as usize);

    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());

    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let v = (clamped * i16::MAX as f32) as i16;
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

/// Re-encodes a WAV buffer with every sample scaled by `volume` (clamped to
/// 0.0-1.0). Used to apply the user's volume slider at serve time, on top
/// of a cached canonical clip, without needing to re-run synthesis or the
/// effects chain. Returns `bytes` unchanged if it's too short to have a
/// full header (mirrors `read_wav_duration_ms`'s defensiveness).
fn apply_volume(bytes: &[u8], volume: f32) -> Vec<u8> {
    if bytes.len() < 44 {
        return bytes.to_vec();
    }
    let volume = volume.clamp(0.0, 1.0);
    let sample_rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let samples: Vec<f32> = bytes[44..]
        .chunks_exact(2)
        .map(|c| (i16::from_le_bytes([c[0], c[1]]) as f32 / i16::MAX as f32) * volume)
        .collect();
    write_wav_bytes(&samples, sample_rate)
}

/// Reads sample rate + data length back out of a buffer `write_wav_bytes`
/// produced, to recover `duration_ms` for cache hits without needing a
/// sidecar metadata file. Returns `None` if `bytes` is too short to have a
/// full 44-byte header.
fn read_wav_duration_ms(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 44 {
        return None;
    }
    let sample_rate = u32::from_le_bytes(bytes[24..28].try_into().ok()?);
    let data_len = u32::from_le_bytes(bytes[40..44].try_into().ok()?);
    if sample_rate == 0 {
        return None;
    }
    let num_samples = data_len / 2;
    Some((num_samples as u64 * 1000 / sample_rate as u64) as u32)
}

// ---------- Tauri commands ----------

#[tauri::command]
pub async fn synthesize_flavor_line(app: AppHandle, index: u32) -> Option<ReminderAudio> {
    tracing::debug!(index, "flavor line clicked");
    let settings = crate::settings::load_settings(&app);
    if !settings.voice.enabled {
        return None;
    }
    let line = dialogues::FLAVOR_LINES.get(index as usize)?;
    synthesize(
        &app,
        line.for_language(settings.voice.language),
        settings.voice.language,
        settings.voice.volume,
    )
    .await
}

/// English text for every flavor line, in `dialogues::FLAVOR_LINES` order —
/// fetched once by the Renderer at startup so it can keep picking/
/// displaying locally afterward without a round-trip per click, while
/// `dialogues.rs` stays the single source of truth for the content itself.
#[tauri::command]
pub fn get_flavor_lines() -> Vec<String> {
    dialogues::FLAVOR_LINES
        .iter()
        .map(|line| line.en.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique scratch directory under the OS temp dir, cleaned up when
    /// dropped — stands in for a real model directory without pulling in a
    /// tempdir crate this project otherwise has no use for.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "fairy-voice-test-{label}-{:?}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_fake_model_files(dir: &Path, sizes: &[(&str, usize)]) {
        for (name, size) in sizes {
            fs::write(dir.join(name), vec![0u8; *size]).unwrap();
        }
    }

    fn fake_model_sizes() -> Vec<(&'static str, usize)> {
        MODEL_FILES.iter().map(|f| (*f, 128)).collect()
    }

    #[test]
    fn model_dir_ready_false_when_files_missing() {
        let dir = TempDir::new("missing-files");
        assert!(!model_dir_ready(&dir.0));
    }

    #[test]
    fn model_dir_ready_generates_a_manifest_and_succeeds_for_a_preexisting_install() {
        // Simulates an install made before this check existed: files
        // present, no manifest yet. Should self-heal rather than fail.
        let dir = TempDir::new("preexisting");
        write_fake_model_files(&dir.0, &fake_model_sizes());

        assert!(model_dir_ready(&dir.0));
        assert!(dir.0.join(MODEL_MANIFEST_FILE).is_file());
    }

    #[test]
    fn model_dir_ready_true_when_sizes_match_manifest() {
        let dir = TempDir::new("matching");
        write_fake_model_files(&dir.0, &fake_model_sizes());
        write_model_manifest(&dir.0).unwrap();

        assert!(model_dir_ready(&dir.0));
    }

    #[test]
    fn model_dir_ready_false_when_a_file_is_truncated_after_manifest_was_written() {
        // The actual bug this exists for: a file on disk silently ends up
        // smaller than what was recorded when it was last known-good.
        let dir = TempDir::new("truncated");
        write_fake_model_files(&dir.0, &fake_model_sizes());
        write_model_manifest(&dir.0).unwrap();

        let corrupted_file = MODEL_FILES[2]; // vector_estimator.int8.onnx
        fs::write(dir.0.join(corrupted_file), vec![0u8; 64]).unwrap();

        assert!(!model_dir_ready(&dir.0));
    }

    #[test]
    fn model_dir_ready_self_heals_when_manifest_is_unparseable() {
        // An unreadable manifest (rather than a missing one) is treated the
        // same way: there's no old known-good state to fall back to either
        // way, so it's regenerated from whatever's on disk now, same as the
        // preexisting-install case above.
        let dir = TempDir::new("bad-manifest");
        write_fake_model_files(&dir.0, &fake_model_sizes());
        fs::write(dir.0.join(MODEL_MANIFEST_FILE), b"not json").unwrap();

        assert!(model_dir_ready(&dir.0));
    }

    #[test]
    fn wav_header_roundtrip_reports_correct_duration() {
        let sample_rate = 44100u32;
        let samples = vec![0.0f32; sample_rate as usize]; // exactly 1 second
        let bytes = write_wav_bytes(&samples, sample_rate);

        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(read_wav_duration_ms(&bytes), Some(1000));
    }

    #[test]
    fn wav_duration_half_second() {
        let sample_rate = 44100u32;
        let samples = vec![0.0f32; (sample_rate / 2) as usize];
        let bytes = write_wav_bytes(&samples, sample_rate);
        assert_eq!(read_wav_duration_ms(&bytes), Some(500));
    }

    #[test]
    fn wav_duration_none_for_truncated_buffer() {
        assert_eq!(read_wav_duration_ms(&[0u8; 10]), None);
    }

    #[test]
    fn samples_clamp_rather_than_wrap_on_overflow() {
        let bytes = write_wav_bytes(&[2.0, -2.0], 44100);
        let s0 = i16::from_le_bytes([bytes[44], bytes[45]]);
        let s1 = i16::from_le_bytes([bytes[46], bytes[47]]);
        assert_eq!(s0, i16::MAX);
        assert_eq!(s1, -i16::MAX);
    }

    #[test]
    fn apply_volume_halves_sample_amplitude_at_half_volume() {
        let original = write_wav_bytes(&[1.0, -1.0, 0.5], 44100);
        let scaled = apply_volume(&original, 0.5);

        let sample_at =
            |bytes: &[u8], i: usize| i16::from_le_bytes([bytes[44 + i * 2], bytes[45 + i * 2]]);
        // i16::MAX * 0.5 rounds down by 1 due to truncation, not rounding —
        // assert within 1 LSB rather than exact equality.
        assert!((sample_at(&scaled, 0) - i16::MAX / 2).abs() <= 1);
        assert!((sample_at(&scaled, 1) - (-i16::MAX / 2)).abs() <= 1);
    }

    #[test]
    fn apply_volume_zero_produces_silence() {
        let original = write_wav_bytes(&[1.0, -1.0, 0.5], 44100);
        let scaled = apply_volume(&original, 0.0);
        for chunk in scaled[44..].chunks_exact(2) {
            assert_eq!(i16::from_le_bytes([chunk[0], chunk[1]]), 0);
        }
    }

    #[test]
    fn apply_volume_clamps_above_one_to_unchanged_amplitude() {
        // Decode-then-re-encode isn't bit-exact due to float rounding, so
        // compare within 1 LSB rather than asserting byte-for-byte equality.
        let original = write_wav_bytes(&[0.5], 44100);
        let scaled = apply_volume(&original, 5.0);
        let sample = |bytes: &[u8]| i16::from_le_bytes([bytes[44], bytes[45]]);
        assert!((sample(&scaled) - sample(&original)).abs() <= 1);
    }

    #[test]
    fn apply_volume_preserves_sample_rate_and_duration() {
        let original = write_wav_bytes(&vec![0.5f32; 44100], 44100);
        let scaled = apply_volume(&original, 0.3);
        assert_eq!(read_wav_duration_ms(&scaled), Some(1000));
    }

    #[test]
    fn apply_volume_leaves_short_buffers_unchanged() {
        assert_eq!(apply_volume(&[1, 2, 3], 0.5), vec![1, 2, 3]);
    }

    #[test]
    fn cache_key_differs_by_language_for_same_text() {
        let en = cache_key("model-a", "hello", VoiceLanguage::En);
        let ja = cache_key("model-a", "hello", VoiceLanguage::Ja);
        assert_ne!(en, ja);
    }

    #[test]
    fn cache_key_is_stable_for_same_input() {
        assert_eq!(
            cache_key(
                "model-a",
                "Time to drink some water, master.",
                VoiceLanguage::En
            ),
            cache_key(
                "model-a",
                "Time to drink some water, master.",
                VoiceLanguage::En
            )
        );
    }

    #[test]
    fn cache_key_differs_by_model_version_for_same_text_and_language() {
        // A future model bump must invalidate old cache entries rather than
        // silently keep serving audio synthesized by the previous model.
        let old_model = cache_key("model-a", "hello", VoiceLanguage::En);
        let new_model = cache_key("model-b", "hello", VoiceLanguage::En);
        assert_ne!(old_model, new_model);
    }

    #[test]
    fn limit_peaks_scales_down_when_above_ceiling() {
        let mut samples = vec![0.5, -1.045, 0.2];
        limit_peaks(&mut samples, 0.98);
        let max_abs = samples.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        assert!((max_abs - 0.98).abs() < 1e-4);
    }

    #[test]
    fn limit_peaks_leaves_signal_untouched_when_under_ceiling() {
        let mut samples = vec![0.5, -0.3, 0.2];
        let before = samples.clone();
        limit_peaks(&mut samples, 0.98);
        assert_eq!(samples, before);
    }

    #[test]
    fn verify_checksum_accepts_matching_hash() {
        let bytes = b"hello world";
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let expected = to_hex(&hasher.finalize());
        assert!(verify_checksum(bytes, &expected).is_ok());
    }

    #[test]
    fn verify_checksum_rejects_mismatched_hash() {
        assert!(verify_checksum(
            b"hello world",
            "0000000000000000000000000000000000000000000000000000000000000000"
        )
        .is_err());
    }
}
