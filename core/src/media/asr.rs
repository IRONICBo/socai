use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use bzip2::read::BzDecoder;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::Archive;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

const MODEL_ID: &str = "sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25";
const MODEL_ARCHIVE: &str = "sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25.tar.bz2";
const MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25.tar.bz2";
const MODEL_SHA256: &str = "393f8a14e2f5fb96746aaab342997a40641001fbd5bf9592a080a8329178ee96";
const MODEL_ARCHIVE_BYTES: u64 = 878_702_423;
const VAD_FILE: &str = "silero_vad.onnx";
const VAD_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx";
const VAD_SHA256: &str = "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6";
const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct AsrModelStatus {
    pub model: &'static str,
    pub runtime: &'static str,
    pub installed: bool,
    pub helper_available: bool,
    pub available: bool,
    pub model_dir: String,
    pub helper_path: Option<String>,
    pub missing_files: Vec<String>,
    pub archive_bytes: u64,
    pub license: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct AsrInstallProgress {
    pub stage: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Clone)]
struct ModelPaths {
    root: PathBuf,
    conv_frontend: PathBuf,
    encoder: PathBuf,
    decoder: PathBuf,
    tokenizer: PathBuf,
    vad: PathBuf,
}

impl ModelPaths {
    fn from_root(root: PathBuf) -> Self {
        Self {
            conv_frontend: root.join("conv_frontend.onnx"),
            encoder: root.join("encoder.int8.onnx"),
            decoder: root.join("decoder.int8.onnx"),
            tokenizer: root.join("tokenizer"),
            vad: root.join(VAD_FILE),
            root,
        }
    }

    fn missing_files(&self) -> Vec<String> {
        let mut missing = Vec::new();
        for path in [&self.conv_frontend, &self.encoder, &self.decoder, &self.vad] {
            if !path.is_file() || std::fs::metadata(path).is_ok_and(|meta| meta.len() == 0) {
                missing.push(relative_display(&self.root, path));
            }
        }
        let tokenizer_ready = self.tokenizer.is_dir()
            && std::fs::read_dir(&self.tokenizer)
                .ok()
                .and_then(|mut entries| entries.next())
                .is_some();
        if !tokenizer_ready {
            missing.push("tokenizer/".into());
        }
        missing
    }
}

#[derive(Serialize)]
struct WorkerRequest<'a> {
    protocol: u32,
    id: u64,
    path: &'a str,
    max_seconds: u64,
}

#[derive(Deserialize)]
struct WorkerResponse {
    protocol: u32,
    id: u64,
    ok: bool,
    transcript: Option<String>,
    error: Option<String>,
}

struct AsrWorker {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    next_id: u64,
}

static WORKER: OnceLock<tokio::sync::Mutex<Option<AsrWorker>>> = OnceLock::new();

pub fn asr_model_status() -> Result<AsrModelStatus> {
    let paths = model_paths()?;
    let missing_files = paths.missing_files();
    let helper_path = find_asr_helper();
    let installed = missing_files.is_empty();
    let helper_available = helper_path.is_some();
    Ok(AsrModelStatus {
        model: "Qwen3-ASR-0.6B-Int8",
        runtime: "local/sherpa-onnx",
        installed,
        helper_available,
        available: installed && helper_available,
        model_dir: paths.root.display().to_string(),
        helper_path: helper_path.map(|path| path.display().to_string()),
        missing_files,
        archive_bytes: MODEL_ARCHIVE_BYTES,
        license: "Apache-2.0",
    })
}

pub fn local_asr_available() -> bool {
    asr_model_status().is_ok_and(|status| status.available)
}

pub async fn install_asr_model<F>(mut progress: F) -> Result<AsrModelStatus>
where
    F: FnMut(AsrInstallProgress) + Send,
{
    let status = asr_model_status()?;
    if status.installed {
        progress_event(
            &mut progress,
            "complete",
            MODEL_ARCHIVE_BYTES,
            Some(MODEL_ARCHIVE_BYTES),
        );
        return Ok(status);
    }

    let paths = model_paths()?;
    if paths.root.exists() {
        anyhow::bail!(
            "local ASR model directory is incomplete: {}; missing: {}. Move it aside and run `socai asr install` again",
            paths.root.display(),
            status.missing_files.join(", ")
        );
    }
    let parent = paths
        .root
        .parent()
        .ok_or_else(|| {
            anyhow!(
                "ASR model directory has no parent: {}",
                paths.root.display()
            )
        })?
        .to_path_buf();
    tokio::fs::create_dir_all(&parent).await?;
    let downloads = parent.join("downloads");
    tokio::fs::create_dir_all(&downloads).await?;
    let archive = downloads.join(MODEL_ARCHIVE);

    let archive_ready = archive.is_file()
        && verify_sha256(archive.clone(), MODEL_SHA256)
            .await
            .unwrap_or(false);
    if archive_ready {
        progress_event(
            &mut progress,
            "download",
            MODEL_ARCHIVE_BYTES,
            Some(MODEL_ARCHIVE_BYTES),
        );
    } else {
        if archive.exists() {
            tokio::fs::remove_file(&archive).await.with_context(|| {
                format!("failed to remove invalid ASR archive {}", archive.display())
            })?;
        }
        download_verified(
            MODEL_URL,
            &archive,
            MODEL_SHA256,
            Some(MODEL_ARCHIVE_BYTES),
            "download",
            &mut progress,
        )
        .await?;
    }

    progress_event(&mut progress, "extract", 0, None);
    let staging = parent.join(format!(".{MODEL_ID}.install-{}", uuid::Uuid::new_v4()));
    let archive_for_unpack = archive.clone();
    let staging_for_unpack = staging.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        std::fs::create_dir_all(&staging_for_unpack)?;
        let file = File::open(&archive_for_unpack)?;
        let decoder = BzDecoder::new(file);
        Archive::new(decoder)
            .unpack(&staging_for_unpack)
            .context("failed to unpack Qwen3-ASR model archive")?;
        Ok(())
    })
    .await
    .context("ASR model extraction task panicked")??;

    let staged_root = staging.join(MODEL_ID);
    let staged_paths = ModelPaths::from_root(staged_root.clone());
    let missing_without_vad: Vec<_> = staged_paths
        .missing_files()
        .into_iter()
        .filter(|item| item != VAD_FILE)
        .collect();
    if !missing_without_vad.is_empty() {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        anyhow::bail!(
            "downloaded Qwen3-ASR archive is incomplete; missing: {}",
            missing_without_vad.join(", ")
        );
    }

    let install_result = async {
        download_verified(
            VAD_URL,
            &staged_paths.vad,
            VAD_SHA256,
            None,
            "vad",
            &mut progress,
        )
        .await?;
        if paths.root.exists() {
            anyhow::bail!(
                "ASR model directory appeared during install: {}",
                paths.root.display()
            );
        }
        tokio::fs::rename(&staged_root, &paths.root)
            .await
            .with_context(|| format!("failed to install ASR model at {}", paths.root.display()))?;
        Ok::<(), anyhow::Error>(())
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&staging).await;
    install_result?;
    let _ = tokio::fs::remove_file(&archive).await;

    let status = asr_model_status()?;
    if !status.installed {
        anyhow::bail!(
            "ASR model install completed with missing files: {}",
            status.missing_files.join(", ")
        );
    }
    progress_event(
        &mut progress,
        "complete",
        MODEL_ARCHIVE_BYTES,
        Some(MODEL_ARCHIVE_BYTES),
    );
    Ok(status)
}

pub async fn transcribe_local_file(path: impl AsRef<Path>, max_seconds: u64) -> Result<String> {
    transcribe_local_file_inner(path.as_ref(), max_seconds, None).await
}

pub(crate) async fn transcribe_local_file_with_timeout(
    path: impl AsRef<Path>,
    max_seconds: u64,
    timeout: Duration,
) -> Result<String> {
    transcribe_local_file_inner(path.as_ref(), max_seconds, Some(timeout)).await
}

async fn transcribe_local_file_inner(
    path: &Path,
    max_seconds: u64,
    timeout: Option<Duration>,
) -> Result<String> {
    let status = asr_model_status()?;
    if !status.installed {
        anyhow::bail!(
            "local Qwen3-ASR model is not installed; run `socai asr install` first (missing: {})",
            status.missing_files.join(", ")
        );
    }
    let helper = find_asr_helper()
        .context("local ASR helper is unavailable; reinstall socai or set SOCAI_ASR_HELPER")?;
    let path = path
        .canonicalize()
        .with_context(|| format!("failed to resolve media path {}", path.display()))?;
    let path_text = path
        .to_str()
        .ok_or_else(|| anyhow!("media path is not valid UTF-8: {}", path.display()))?;

    let worker_slot = WORKER.get_or_init(|| tokio::sync::Mutex::new(None));
    let mut slot = worker_slot.lock().await;
    let stopped = match slot.as_mut() {
        Some(worker) => worker.child.try_wait()?.is_some(),
        None => false,
    };
    if stopped {
        *slot = None;
    }
    if slot.is_none() {
        *slot = Some(start_worker(&helper, &status.model_dir).await?);
    }
    let result = match timeout {
        Some(timeout) => match tokio::time::timeout(
            timeout,
            slot.as_mut()
                .expect("worker initialized")
                .transcribe(path_text, max_seconds),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(anyhow!(
                "local Qwen3-ASR timed out after {}s",
                timeout.as_secs()
            )),
        },
        None => {
            slot.as_mut()
                .expect("worker initialized")
                .transcribe(path_text, max_seconds)
                .await
        }
    };
    if result.is_err() {
        // A transport or protocol error can leave a late response in stdout.
        // Drop the worker so the next request starts with a clean protocol stream.
        *slot = None;
    }
    result
}

impl AsrWorker {
    async fn transcribe(&mut self, path: &str, max_seconds: u64) -> Result<String> {
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        let mut request = serde_json::to_vec(&WorkerRequest {
            protocol: PROTOCOL_VERSION,
            id,
            path,
            max_seconds,
        })?;
        request.push(b'\n');
        self.stdin.write_all(&request).await?;
        self.stdin.flush().await?;
        let line = self
            .stdout
            .next_line()
            .await?
            .context("local ASR helper exited without a response")?;
        let response: WorkerResponse = serde_json::from_str(&line)
            .with_context(|| format!("invalid local ASR helper response: {line}"))?;
        if response.protocol != PROTOCOL_VERSION || response.id != id {
            anyhow::bail!(
                "local ASR protocol mismatch: expected v{PROTOCOL_VERSION} request {id}, got v{} request {}",
                response.protocol,
                response.id
            );
        }
        if response.ok {
            response
                .transcript
                .filter(|text| !text.trim().is_empty())
                .context("local ASR helper returned an empty transcript")
        } else {
            anyhow::bail!(
                "{}",
                response.error.unwrap_or_else(|| "local ASR failed".into())
            )
        }
    }
}

async fn start_worker(helper: &Path, model_dir: &str) -> Result<AsrWorker> {
    let mut child = Command::new(helper)
        .arg("--serve")
        .arg("--model-dir")
        .arg(model_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("failed to start local ASR helper {}", helper.display()))?;
    let stdin = child
        .stdin
        .take()
        .context("ASR helper stdin is unavailable")?;
    let stdout = child
        .stdout
        .take()
        .context("ASR helper stdout is unavailable")?;
    Ok(AsrWorker {
        child,
        stdin,
        stdout: BufReader::new(stdout).lines(),
        next_id: 0,
    })
}

fn find_asr_helper() -> Option<PathBuf> {
    let filename = if cfg!(windows) {
        "socai-asr.exe"
    } else {
        "socai-asr"
    };
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("SOCAI_ASR_HELPER") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(dir) = executable.parent() {
            candidates.push(dir.join(filename));
            if dir.file_name().is_some_and(|name| name == "deps") {
                if let Some(profile_dir) = dir.parent() {
                    candidates.push(profile_dir.join(filename));
                }
            }
        }
    }
    if let Some(workspace) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() {
        candidates.push(workspace.join("target").join("debug").join(filename));
        candidates.push(workspace.join("target").join("release").join(filename));
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn model_paths() -> Result<ModelPaths> {
    let root = if let Some(path) = std::env::var_os("SOCAI_ASR_MODEL_DIR") {
        PathBuf::from(path)
    } else if let Some(home) = std::env::var_os("SOCAI_HOME") {
        PathBuf::from(home).join("models").join(MODEL_ID)
    } else {
        dirs::home_dir()
            .context("could not resolve home directory for local ASR model")?
            .join(".socai")
            .join("models")
            .join(MODEL_ID)
    };
    Ok(ModelPaths::from_root(root))
}

async fn download_verified<F>(
    url: &str,
    target: &Path,
    expected_sha256: &str,
    expected_size: Option<u64>,
    stage: &str,
    progress: &mut F,
) -> Result<()>
where
    F: FnMut(AsrInstallProgress) + Send,
{
    let part = target.with_extension(format!(
        "{}.part",
        target
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("download")
    ));
    if part.exists() {
        tokio::fs::remove_file(&part).await?;
    }
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await?
        .error_for_status()?;
    let total = response.content_length().or(expected_size);
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&part).await?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        progress_event(progress, stage, downloaded, total);
    }
    file.flush().await?;
    drop(file);
    let digest = format!("{:x}", hasher.finalize());
    if digest != expected_sha256 {
        let _ = tokio::fs::remove_file(&part).await;
        anyhow::bail!("checksum mismatch for {url}: expected {expected_sha256}, got {digest}");
    }
    tokio::fs::rename(&part, target).await?;
    Ok(())
}

async fn verify_sha256(path: PathBuf, expected: &'static str) -> Result<bool> {
    tokio::task::spawn_blocking(move || -> Result<bool> {
        let mut file = File::open(&path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(format!("{:x}", hasher.finalize()) == expected)
    })
    .await
    .context("ASR checksum task panicked")?
}

fn progress_event<F>(progress: &mut F, stage: &str, downloaded_bytes: u64, total_bytes: Option<u64>)
where
    F: FnMut(AsrInstallProgress),
{
    progress(AsrInstallProgress {
        stage: stage.to_string(),
        downloaded_bytes,
        total_bytes,
    });
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
