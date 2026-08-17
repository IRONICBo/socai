use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use base64::Engine;
use fs2::FileExt;
use futures::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, State};
use tokio::io::AsyncWriteExt;

const MODEL_DOWNLOAD_EVENT: &str = "whisper:model-progress";
const MAX_WAV_BYTES: usize = 12 * 1024 * 1024;
const TRANSCRIPTION_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WhisperModelSize {
    Low,
    Medium,
    High,
}

impl WhisperModelSize {
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            _ => anyhow::bail!("Whisper model size must be one of: low, medium, high"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    fn spec(self) -> ModelSpec {
        match self {
            Self::Low => ModelSpec {
                model_name: "base",
                filename: "ggml-base.bin",
                expected_bytes: 147_951_465,
                sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
            },
            Self::Medium => ModelSpec {
                model_name: "small",
                filename: "ggml-small.bin",
                expected_bytes: 487_601_967,
                sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
            },
            Self::High => ModelSpec {
                model_name: "medium",
                filename: "ggml-medium.bin",
                expected_bytes: 1_533_763_059,
                sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ModelSpec {
    model_name: &'static str,
    filename: &'static str,
    expected_bytes: u64,
    sha256: &'static str,
}

impl ModelSpec {
    fn url(self) -> String {
        format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            self.filename
        )
    }
}

#[derive(Debug, Default)]
struct WhisperRuntimeState {
    downloading_size: Option<String>,
    downloaded_bytes: u64,
    total_bytes: u64,
    last_error: Option<String>,
    verified_models: HashMap<PathBuf, ModelFingerprint>,
}

pub struct WhisperState {
    download_lock: tokio::sync::Mutex<()>,
    runtime: Mutex<WhisperRuntimeState>,
}

impl Default for WhisperState {
    fn default() -> Self {
        prepare_private_directories();
        scavenge_stale_files();
        Self {
            download_lock: tokio::sync::Mutex::new(()),
            runtime: Mutex::new(WhisperRuntimeState::default()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ModelFingerprint {
    bytes: u64,
    modified_nanos: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct WhisperStatus {
    ready: bool,
    state: String,
    binary_available: bool,
    binary_path: Option<String>,
    model_available: bool,
    model_size: String,
    model_name: String,
    model_path: String,
    model_bytes: u64,
    downloaded_bytes: u64,
    total_bytes: u64,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WhisperTranscript {
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct WhisperProgress {
    model_size: String,
    downloaded_bytes: u64,
    total_bytes: u64,
}

#[tauri::command]
pub fn whisper_status(state: State<'_, WhisperState>) -> WhisperStatus {
    status_snapshot(&state)
}

#[tauri::command]
pub async fn whisper_select_model(
    app: AppHandle,
    state: State<'_, WhisperState>,
    size: String,
) -> Result<WhisperStatus, String> {
    let size = WhisperModelSize::parse(&size).map_err(|err| format!("{err:#}"))?;
    socai_core::config::set_config_key("whisper.model_size", size.as_str())
        .map_err(|err| format!("{err:#}"))?;
    clear_runtime_error(&state);
    ensure_model(&app, &state, size).await?;
    Ok(status_snapshot(&state))
}

#[tauri::command]
pub async fn whisper_transcribe(
    state: State<'_, WhisperState>,
    audio_base64: String,
) -> Result<WhisperTranscript, String> {
    let status = status_snapshot(&state);
    if !status.binary_available {
        return Err("local Whisper executable is not installed".into());
    }
    if !status.model_available {
        return Err("the selected Whisper model is not available".into());
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_base64.as_bytes())
        .map_err(|_| "recorded audio is not valid base64".to_string())?;
    validate_wav(&bytes)?;

    let temp_dir = whisper_root().join("tmp");
    create_private_dir(&temp_dir)
        .map_err(|err| format!("could not create Whisper temporary directory: {err:#}"))?;
    let input_path = temp_dir.join(format!("voice-{}-{}.wav", std::process::id(), unix_nanos()));
    let mut input_guard = TemporaryFile::new(input_path.clone());
    let mut input_options = std::fs::OpenOptions::new();
    input_options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        input_options.mode(0o600);
    }
    let input_file = input_options
        .open(&input_path)
        .map_err(|err| format!("could not create recorded audio file: {err}"))?;
    let mut input_file = tokio::fs::File::from_std(input_file);
    input_file
        .write_all(&bytes)
        .await
        .map_err(|err| format!("could not save recorded audio: {err}"))?;
    input_file
        .flush()
        .await
        .map_err(|err| format!("could not flush recorded audio: {err}"))?;
    drop(input_file);

    let binary = status
        .binary_path
        .as_deref()
        .ok_or_else(|| "local Whisper executable is not installed".to_string())?;
    let mut command = tokio::process::Command::new(binary);
    command
        .arg("--model")
        .arg(&status.model_path)
        .arg("--file")
        .arg(&input_path)
        .arg("--language")
        .arg("auto")
        .arg("--no-timestamps")
        .arg("--no-prints")
        .stdin(Stdio::null())
        .kill_on_drop(true);

    let output = tokio::time::timeout(TRANSCRIPTION_TIMEOUT, command.output())
        .await
        .map_err(|_| "local Whisper transcription timed out".to_string())?
        .map_err(|err| format!("could not start local Whisper: {err}"))?;
    input_guard.remove_now();

    if !output.status.success() {
        let detail = bounded_error(&String::from_utf8_lossy(&output.stderr));
        return Err(format!(
            "local Whisper exited with {}{}",
            output.status,
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err("local Whisper returned no speech".into());
    }
    Ok(WhisperTranscript { text })
}

async fn ensure_model(
    app: &AppHandle,
    state: &WhisperState,
    size: WhisperModelSize,
) -> Result<(), String> {
    let _guard = state.download_lock.lock().await;
    create_private_dir(&whisper_root())
        .map_err(|err| format!("could not create Whisper model directory: {err:#}"))?;
    let model_lock = tokio::task::spawn_blocking(ModelDownloadLock::acquire)
        .await
        .map_err(|err| format!("could not join Whisper model lock task: {err}"))?
        .map_err(|err| format!("could not lock Whisper model directory: {err:#}"))?;
    let spec = size.spec();
    let target = model_path(spec);
    if model_is_verified(state, &target, spec) {
        clear_runtime_error(state);
        return Ok(());
    }

    set_download_progress(state, size, 0, spec.expected_bytes, None);
    emit_progress(app, size, 0, spec.expected_bytes);
    let result = download_model(app, state, size, spec, &target).await;
    match &result {
        Ok(()) => {
            mark_model_verified(state, &target);
            set_download_progress(state, size, spec.expected_bytes, spec.expected_bytes, None);
            emit_progress(app, size, spec.expected_bytes, spec.expected_bytes);
            if let Ok(mut runtime) = state.runtime.lock() {
                runtime.downloading_size = None;
            }
        }
        Err(error) => {
            set_download_progress(state, size, 0, spec.expected_bytes, Some(error.clone()));
            if let Ok(mut runtime) = state.runtime.lock() {
                runtime.downloading_size = None;
            }
            emit_progress(app, size, 0, spec.expected_bytes);
        }
    }
    drop(model_lock);
    result
}

async fn download_model(
    app: &AppHandle,
    state: &WhisperState,
    size: WhisperModelSize,
    spec: ModelSpec,
    target: &Path,
) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or_else(|| "Whisper model path has no parent directory".to_string())?;
    create_private_dir(parent)
        .map_err(|err| format!("could not create Whisper model directory: {err:#}"))?;

    let part_path = parent.join(format!("{}.{}.part", spec.filename, unix_nanos()));
    let mut partial = TemporaryFile::new(part_path.clone());
    let client = reqwest::Client::builder()
        .user_agent(format!("socai/{}", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|err| format!("could not create Whisper download client: {err}"))?;
    let response = client
        .get(spec.url())
        .send()
        .await
        .map_err(|err| format!("could not download Whisper model: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Whisper model download failed: {err}"))?;
    if let Some(length) = response.content_length() {
        if length != spec.expected_bytes {
            return Err(format!(
                "Whisper model size changed unexpectedly (expected {}, got {length})",
                spec.expected_bytes
            ));
        }
    }

    let mut part_options = std::fs::OpenOptions::new();
    part_options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        part_options.mode(0o600);
    }
    let part_file = part_options
        .open(&part_path)
        .map_err(|err| format!("could not create Whisper model file: {err}"))?;
    let mut file = tokio::fs::File::from_std(part_file);
    let mut stream = response.bytes_stream();
    let mut digest = Sha256::new();
    let mut downloaded = 0_u64;
    let mut last_reported = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| format!("Whisper model download interrupted: {err}"))?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > spec.expected_bytes {
            return Err("Whisper model download exceeded its expected size".into());
        }
        digest.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|err| format!("could not write Whisper model: {err}"))?;
        if downloaded.saturating_sub(last_reported) >= 2 * 1024 * 1024 {
            last_reported = downloaded;
            set_download_progress(state, size, downloaded, spec.expected_bytes, None);
            emit_progress(app, size, downloaded, spec.expected_bytes);
        }
    }
    file.flush()
        .await
        .map_err(|err| format!("could not flush Whisper model: {err}"))?;
    file.sync_all()
        .await
        .map_err(|err| format!("could not sync Whisper model: {err}"))?;
    drop(file);

    if downloaded != spec.expected_bytes {
        return Err(format!(
            "Whisper model download is incomplete (expected {}, got {downloaded})",
            spec.expected_bytes
        ));
    }
    let actual_hash = hex::encode(digest.finalize());
    if actual_hash != spec.sha256 {
        return Err("Whisper model checksum verification failed".into());
    }

    #[cfg(windows)]
    if tokio::fs::try_exists(target).await.unwrap_or(false) {
        tokio::fs::remove_file(target)
            .await
            .map_err(|err| format!("could not replace the existing Whisper model: {err}"))?;
    }
    tokio::fs::rename(&part_path, target)
        .await
        .map_err(|err| format!("could not install the Whisper model: {err}"))?;
    partial.disarm();
    Ok(())
}

fn status_snapshot(state: &WhisperState) -> WhisperStatus {
    let size = selected_model_size();
    let spec = size.spec();
    let binary = resolve_whisper_binary();
    let path = model_path(spec);
    let model_available = model_is_verified(state, &path, spec);
    let runtime = state.runtime.lock().ok();
    let downloading = runtime
        .as_ref()
        .and_then(|value| value.downloading_size.as_deref())
        == Some(size.as_str());
    let error = runtime.as_ref().and_then(|value| value.last_error.clone());
    let state_name = if downloading {
        "downloading"
    } else if error.is_some() {
        "error"
    } else if binary.is_none() {
        "binary_missing"
    } else if !model_available {
        "model_missing"
    } else {
        "ready"
    };
    WhisperStatus {
        ready: binary.is_some() && model_available && !downloading,
        state: state_name.into(),
        binary_available: binary.is_some(),
        binary_path: binary.map(|value| value.to_string_lossy().into_owned()),
        model_available,
        model_size: size.as_str().into(),
        model_name: spec.model_name.into(),
        model_path: path.to_string_lossy().into_owned(),
        model_bytes: spec.expected_bytes,
        downloaded_bytes: runtime.as_ref().map_or(0, |value| value.downloaded_bytes),
        total_bytes: runtime.as_ref().map_or(spec.expected_bytes, |value| {
            value.total_bytes.max(spec.expected_bytes)
        }),
        error,
    }
}

fn selected_model_size() -> WhisperModelSize {
    socai_core::config::load_config()
        .ok()
        .and_then(|config| config.whisper.model_size)
        .and_then(|value| WhisperModelSize::parse(&value).ok())
        .unwrap_or(WhisperModelSize::Medium)
}

fn whisper_root() -> PathBuf {
    socai_core::agent::file_bash_tools::socai_home_dir().join("models/whisper")
}

fn model_path(spec: ModelSpec) -> PathBuf {
    whisper_root().join(spec.filename)
}

fn model_is_verified(state: &WhisperState, path: &Path, spec: ModelSpec) -> bool {
    let Some(fingerprint) = model_fingerprint(path, spec.expected_bytes) else {
        return false;
    };
    if state
        .runtime
        .lock()
        .is_ok_and(|runtime| runtime.verified_models.get(path) == Some(&fingerprint))
    {
        return true;
    }
    let verified = sha256_file(path).is_ok_and(|hash| hash == spec.sha256);
    if verified {
        if let Ok(mut runtime) = state.runtime.lock() {
            runtime
                .verified_models
                .insert(path.to_path_buf(), fingerprint);
        }
    }
    verified
}

fn mark_model_verified(state: &WhisperState, path: &Path) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    let fingerprint = ModelFingerprint {
        bytes: metadata.len(),
        modified_nanos: modified_nanos(&metadata),
    };
    if let Ok(mut runtime) = state.runtime.lock() {
        runtime
            .verified_models
            .insert(path.to_path_buf(), fingerprint);
    }
}

fn model_fingerprint(path: &Path, expected_bytes: u64) -> Option<ModelFingerprint> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() != expected_bytes {
        return None;
    }
    Some(ModelFingerprint {
        bytes: metadata.len(),
        modified_nanos: modified_nanos(&metadata),
    })
}

fn modified_nanos(metadata: &std::fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos())
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn resolve_whisper_binary() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = std::env::var_os("SOCAI_WHISPER_CLI") {
        let path = PathBuf::from(value);
        if path.components().count() > 1 || path.is_absolute() {
            candidates.push(path);
        } else {
            candidates.extend(path_candidates(&path));
        }
    }
    for name in binary_names() {
        candidates.extend(path_candidates(Path::new(name)));
    }
    #[cfg(target_os = "macos")]
    {
        candidates.push(PathBuf::from("/opt/homebrew/bin/whisper-cli"));
        candidates.push(PathBuf::from("/usr/local/bin/whisper-cli"));
    }
    let mut seen = HashSet::new();
    candidates.into_iter().find(|path| {
        seen.insert(path.clone())
            && std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
    })
}

fn path_candidates(name: &Path) -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|value| {
            std::env::split_paths(&value)
                .map(|dir| dir.join(name))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn binary_names() -> &'static [&'static str] {
    &["whisper-cli.exe", "whisper-cpp.exe"]
}

#[cfg(not(target_os = "windows"))]
fn binary_names() -> &'static [&'static str] {
    &["whisper-cli", "whisper-cpp"]
}

fn validate_wav(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 44 || bytes.len() > MAX_WAV_BYTES {
        return Err("recorded audio has an invalid size".into());
    }
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err("recorded audio is not a WAV file".into());
    }
    Ok(())
}

fn set_download_progress(
    state: &WhisperState,
    size: WhisperModelSize,
    downloaded_bytes: u64,
    total_bytes: u64,
    error: Option<String>,
) {
    if let Ok(mut runtime) = state.runtime.lock() {
        runtime.downloading_size = Some(size.as_str().into());
        runtime.downloaded_bytes = downloaded_bytes;
        runtime.total_bytes = total_bytes;
        runtime.last_error = error;
    }
}

fn clear_runtime_error(state: &WhisperState) {
    if let Ok(mut runtime) = state.runtime.lock() {
        runtime.last_error = None;
    }
}

fn emit_progress(app: &AppHandle, size: WhisperModelSize, downloaded_bytes: u64, total_bytes: u64) {
    let _ = app.emit(
        MODEL_DOWNLOAD_EVENT,
        WhisperProgress {
            model_size: size.as_str().into(),
            downloaded_bytes,
            total_bytes,
        },
    );
}

fn bounded_error(value: &str) -> String {
    let trimmed = value.trim();
    let mut chars = trimmed.chars();
    let bounded: String = chars.by_ref().take(2_000).collect();
    if chars.next().is_some() {
        format!("{bounded}…")
    } else {
        bounded
    }
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn prepare_private_directories() {
    let root = whisper_root();
    let _ = create_private_dir(&root);
    let _ = create_private_dir(&root.join("tmp"));
}

fn create_private_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("failed to secure {}", path.display()))?;
    }
    Ok(())
}

fn scavenge_stale_files() {
    let root = whisper_root();
    remove_stale_matching(&root, Duration::from_secs(24 * 60 * 60), |name| {
        name.ends_with(".part")
    });
    remove_stale_matching(&root.join("tmp"), Duration::from_secs(10 * 60), |name| {
        name.starts_with("voice-") && name.ends_with(".wav")
    });
}

fn remove_stale_matching(directory: &Path, max_age: Duration, matches: impl Fn(&str) -> bool) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let stale = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age >= max_age);
        if matches(&name) && stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

struct ModelDownloadLock(File);

impl ModelDownloadLock {
    fn acquire() -> Result<Self> {
        let root = whisper_root();
        create_private_dir(&root)?;
        let lock_path = root.join(".download.lock");
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&lock_path)
            .with_context(|| format!("failed to open {}", lock_path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("failed to lock {}", lock_path.display()))?;
        Ok(Self(file))
    }
}

impl Drop for ModelDownloadLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

struct TemporaryFile {
    path: Option<PathBuf>,
}

impl TemporaryFile {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn disarm(&mut self) {
        self.path = None;
    }

    fn remove_now(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        self.remove_now();
    }
}
