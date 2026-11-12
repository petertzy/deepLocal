//! Optional OPUS-MT model installation outside the application bundle.
use crate::ApiState;
use axum::{extract::State, http::{HeaderMap, StatusCode, header::ORIGIN}, response::IntoResponse, Json};
use chrono::Utc;
use deeplocal_core::DownloadJob;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io::Read, path::{Path, PathBuf}, sync::Arc, time::Instant};
use tokio::{io::AsyncWriteExt, sync::{Mutex, RwLock}};
use uuid::Uuid;

const REQUIRED_FILES: &[&str] = &[
    "model.bin", "source.spm", "target.spm", "config.json", "shared_vocabulary.json",
    "README.md",
];
const RECEIPT: &str = ".deeplocal-opus-mt.json";

// Browser pages must not be able to trigger model downloads or deletions.
fn trusted_ui_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(ORIGIN) else { return true; };
    matches!(origin.to_str().ok(), Some(
        "tauri://localhost"
        | "http://tauri.localhost"
        | "http://127.0.0.1:5173"
        | "http://localhost:5173"
    ))
}

fn model_source(direction: &str) -> Option<(&'static str, &'static str)> {
    match direction {
        "en-zh" => Some(("English to Chinese", "Sams200/opus-mt-en-zh")),
        "zh-en" => Some(("Chinese to English", "Sams200/opus-mt-zh-en")),
        _ => None,
    }
}

fn model_path(root: &Path, direction: &str) -> Option<PathBuf> {
    model_source(direction).map(|_| root.join("opus-mt").join(direction))
}

fn has_model_files(dir: &Path) -> bool {
    ["model.bin", "source.spm", "target.spm", "config.json", "shared_vocabulary.json"]
        .iter()
        .all(|file| dir.join(file).is_file())
}

#[derive(Deserialize)]
pub(crate) struct ModelDirection {
    direction: String,
}

#[derive(Deserialize)]
struct HubRevision {
    sha: String,
}

#[derive(Deserialize)]
struct HubFile {
    path: String,
    size: Option<u64>,
    lfs: Option<HubLfs>,
}

#[derive(Deserialize)]
struct HubLfs {
    oid: Option<String>,
    sha256: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct InstalledFile {
    filename: String,
    sha256: String,
    size_bytes: u64,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    repository: String,
    revision: String,
    files: Vec<InstalledFile>,
}

fn digest_file(path: &Path) -> anyhow::Result<(String, u64)> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut data = [0u8; 1024 * 1024];
    let mut len = 0;
    loop {
        let count = file.read(&mut data)?;
        if count == 0 { break; }
        hash.update(&data[..count]);
        len += count as u64;
    }
    Ok((format!("{:x}", hash.finalize()), len))
}

fn check_receipt(dir: &Path) -> anyhow::Result<bool> {
    if !has_model_files(dir) { return Ok(false); }
    let receipt: Receipt = serde_json::from_slice(&std::fs::read(dir.join(RECEIPT))?)?;
    if receipt.files.len() != REQUIRED_FILES.len() { anyhow::bail!("Incomplete model manifest"); }
    for name in REQUIRED_FILES {
        let entry = receipt.files.iter().find(|item| item.filename == *name)
            .ok_or_else(|| anyhow::anyhow!("Missing manifest entry for {name}"))?;
        let (actual_hash, actual_size) = digest_file(&dir.join(name))?;
        if actual_hash != entry.sha256 || actual_size != entry.size_bytes {
            anyhow::bail!("Integrity verification failed for {name}");
        }
    }
    Ok(true)
}

pub(crate) async fn list_models(
    State(state): State<Arc<ApiState>>,
) -> Json<serde_json::Value> {
    let mut models = Vec::new();
    for direction in ["en-zh", "zh-en"] {
        let (label, repo) = model_source(direction).expect("known direction");
        let dir = model_path(&state.models_directory, direction).expect("known direction");
        let installed = has_model_files(&dir);
        let size_bytes: u64 = REQUIRED_FILES.iter().filter_map(|name|
            std::fs::metadata(dir.join(name)).ok().map(|metadata| metadata.len())
        ).sum();
        models.push(serde_json::json!({
            "direction": direction,
            "name": label,
            "repository": repo,
            "installed": installed,
            "directory_exists": dir.exists(),
            "size_bytes": (size_bytes > 0).then_some(size_bytes),
            "verified_manifest": installed && dir.join(RECEIPT).is_file(),
            "directory": dir.to_string_lossy()
        }));
    }
    Json(serde_json::json!({
        "models": models,
        "runtime": {
            "provider": "external-python",
            "configured": std::env::var("DEEPLOCAL_OPUS_MT_PYTHON").is_ok(),
            "bundled": false
        }
    }))
}

pub(crate) async fn check_runtime(headers: HeaderMap) -> impl IntoResponse {
    if !trusted_ui_origin(&headers) { return StatusCode::FORBIDDEN.into_response(); }
    let python = std::env::var("DEEPLOCAL_OPUS_MT_PYTHON").unwrap_or_else(|_| "python3".into());
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::process::Command::new(&python)
            .arg("-c")
            .arg("import ctranslate2, sentencepiece")
            .kill_on_drop(true)
            .output(),
    ).await;
    match result {
        Ok(Ok(output)) if output.status.success() =>
            Json(serde_json::json!({"ready": true, "message": "Python translation dependencies are available."})).into_response(),
        _ => Json(serde_json::json!({
            "ready": false,
            "message": "Install Python, CTranslate2 and SentencePiece, then set DEEPLOCAL_OPUS_MT_PYTHON to the interpreter path before starting deepLocal."
        })).into_response()
    }
}

pub(crate) async fn install_model(
    headers: HeaderMap,
    State(state): State<Arc<ApiState>>,
    Json(request): Json<ModelDirection>,
) -> impl IntoResponse {
    if !trusted_ui_origin(&headers) { return StatusCode::FORBIDDEN.into_response(); }
    let Some((_, repo)) = model_source(&request.direction) else {
        return (StatusCode::BAD_REQUEST, "Unsupported OPUS-MT direction").into_response();
    };
    let target = model_path(&state.models_directory, &request.direction).expect("known direction");
    if target.exists() {
        return (StatusCode::CONFLICT, "Model directory already exists. Delete the existing installation first.").into_response();
    }
    let job_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let job = DownloadJob {
        id: job_id.clone(),
        repo: repo.into(),
        filename: format!("opus-mt-{}", request.direction),
        status: "queued".into(),
        downloaded_bytes: 0,
        total_bytes: None,
        speed_bytes_per_sec: None,
        eta_seconds: None,
        local_path: Some(target.to_string_lossy().into_owned()),
        error: None,
        cancel_requested: false,
        created_at: now,
        updated_at: now,
    };
    {
        let mut jobs = state.downloads.write().await;
        if jobs.values().any(|job| job.filename == format!("opus-mt-{}", request.direction)
            && matches!(job.status.as_str(), "queued" | "downloading" | "cancelling")) {
            return (StatusCode::CONFLICT, "A download is already active for this direction").into_response();
        }
        jobs.insert(job_id.clone(), job.clone());
    }
    super::persist_download_job(&state.storage, &job).await;
    let worker_job_id = job_id.clone();
    tokio::spawn(async move {
        let job_id = worker_job_id;
        let result = download_direction(&state, &job_id, repo, &request.direction, &target).await;
        let (status, error) = match result {
            Ok(()) => ("downloaded", None),
            Err(e) if e.to_string() == "cancelled" => ("cancelled", None),
            Err(e) => ("error", Some(e.to_string())),
        };
        update_job(&state.downloads, &state.storage, &job_id, status, None, None, error).await;
    });
    (StatusCode::ACCEPTED, Json(serde_json::json!({"job_id": job_id}))).into_response()
}

async fn update_job(
    downloads: &Arc<RwLock<HashMap<String, DownloadJob>>>,
    storage: &Arc<Mutex<deeplocal_storage::Storage>>,
    id: &str,
    status: &str,
    bytes: Option<u64>,
    total: Option<u64>,
    error: Option<String>,
) {
    let mut jobs = downloads.write().await;
    if let Some(job) = jobs.get_mut(id) {
        job.status = status.into();
        if let Some(bytes) = bytes { job.downloaded_bytes = bytes; }
        if let Some(total) = total { job.total_bytes = Some(total); }
        job.error = error;
        job.updated_at = Utc::now();
        let job = job.clone();
        drop(jobs);
        super::persist_download_job(storage, &job).await;
    }
}

async fn download_direction(
    state: &Arc<ApiState>,
    job_id: &str,
    repo: &str,
    direction: &str,
    target: &Path,
) -> anyhow::Result<()> {
    let parent = target.parent().ok_or_else(|| anyhow::anyhow!("Invalid model directory"))?;
    tokio::fs::create_dir_all(parent).await?;
    let root = state.models_directory.canonicalize()?;
    let canonical_parent = parent.canonicalize()?;
    if canonical_parent != root.join("opus-mt") {
        anyhow::bail!("Translation model folder escapes the configured models root");
    }
    let staging = parent.join(format!(".install-{direction}-{job_id}"));
    tokio::fs::create_dir(&staging).await?;
    let result = download_into_stage(state, job_id, repo, target, &staging).await;
    if result.is_err() {
        tokio::fs::remove_dir_all(&staging).await.ok();
    }
    result
}

// A local fixture source is available only in debug builds for integration tests.
// Release builds always download from the fixed public Hugging Face host.
fn hub_base_url() -> String {
    #[cfg(debug_assertions)]
    if let Ok(value) = std::env::var("DEEPLOCAL_OPUS_MT_TEST_HUB_URL") {
        if let Ok(url) = reqwest::Url::parse(&value) {
            if url.scheme() == "http"
                && url.host_str() == Some("127.0.0.1")
                && url.port().is_some()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none()
                && url.username().is_empty()
                && url.password().is_none()
            {
                return value.trim_end_matches('/').to_owned();
            }
        }
    }
    "https://huggingface.co".to_owned()
}

async fn download_into_stage(
    state: &Arc<ApiState>,
    job_id: &str,
    repo: &str,
    target: &Path,
    staging: &Path,
) -> anyhow::Result<()> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(900)).build()?;
    let hub = hub_base_url();
    let revision: HubRevision = client.get(format!("{hub}/api/models/{repo}"))
        .send().await?.error_for_status()?.json().await?;
    if revision.sha.len() != 40 || !revision.sha.bytes().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!("Invalid model repository revision");
    }
    let entries: Vec<HubFile> = client.get(format!("{hub}/api/models/{repo}/tree/{}", revision.sha))
        .send().await?.error_for_status()?.json().await?;
    let mut expected = Vec::new();
    for name in REQUIRED_FILES {
        let meta = entries.iter().find(|file| file.path == *name)
            .ok_or_else(|| anyhow::anyhow!("Missing {name} in model repository"))?;
        let hash = meta.lfs.as_ref().and_then(|lfs| lfs.sha256.as_ref().or(lfs.oid.as_ref()));
        let hash = hash.filter(|value| value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit()))
            .cloned();
        if name == &"model.bin" && hash.is_none() {
            anyhow::bail!("The model host did not provide a SHA-256 checksum for model.bin");
        }
        expected.push((name.to_string(), hash, meta.size.unwrap_or(0)));
    }
    let total: u64 = expected.iter().map(|(_, _, size)| *size).sum();
    update_job(&state.downloads, &state.storage, job_id, "downloading", Some(0),
        (total > 0).then_some(total), None).await;
    let mut transferred = 0u64;
    let start = Instant::now();
    let mut manifest_files = Vec::new();
    for (filename, expected_hash, expected_size) in expected {
        if super::download_cancel_requested(&state.downloads, job_id).await {
            anyhow::bail!("cancelled");
        }
        let url = format!("{hub}/{repo}/resolve/{}/{}",
            revision.sha, filename);
        let response = client.get(url).send().await?.error_for_status()?;
        let mut stream = response.bytes_stream();
        let file_path = staging.join(&filename);
        let mut file = tokio::fs::File::create(&file_path).await?;
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        while let Some(chunk) = stream.next().await {
            if super::download_cancel_requested(&state.downloads, job_id).await {
                anyhow::bail!("cancelled");
            }
            let chunk = chunk?;
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            received += chunk.len() as u64;
            transferred += chunk.len() as u64;
            let elapsed = start.elapsed().as_secs_f64();
            let speed = (elapsed > 0.0).then_some(transferred as f64 / elapsed);
            let mut jobs = state.downloads.write().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.downloaded_bytes = transferred;
                job.speed_bytes_per_sec = speed;
                job.eta_seconds = if total > transferred {
                    speed.filter(|v| *v > 0.0).map(|v| ((total - transferred) as f64 / v).ceil() as u64)
                } else { Some(0) };
                job.updated_at = Utc::now();
            }
        }
        file.flush().await?;
        drop(file);
        if expected_size > 0 && received != expected_size {
            anyhow::bail!("Size mismatch for {filename}: expected {expected_size}, received {received}");
        }
        let actual_hash = format!("{:x}", hasher.finalize());
        if let Some(expected_hash) = expected_hash {
            if actual_hash != expected_hash.to_ascii_lowercase() {
                anyhow::bail!("SHA-256 verification failed for {filename}");
            }
        }
        manifest_files.push(InstalledFile { filename, sha256: actual_hash, size_bytes: received });
        update_job(&state.downloads, &state.storage, job_id, "downloading",
            Some(transferred), (total > 0).then_some(total), None).await;
    }
    tokio::fs::write(staging.join(RECEIPT),
        serde_json::to_vec_pretty(&Receipt {
            repository: repo.to_string(),
            revision: revision.sha,
            files: manifest_files,
        })?,
    ).await?;
    if super::download_cancel_requested(&state.downloads, job_id).await { anyhow::bail!("cancelled"); }
    if target.exists() { anyhow::bail!("Model directory appeared during installation"); }
    tokio::fs::rename(staging, target).await?;
    Ok(())
}

pub(crate) async fn verify_model(
    headers: HeaderMap,
    State(state): State<Arc<ApiState>>,
    Json(request): Json<ModelDirection>,
) -> impl IntoResponse {
    if !trusted_ui_origin(&headers) { return StatusCode::FORBIDDEN.into_response(); }
    let Some(dir) = model_path(&state.models_directory, &request.direction) else {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"verified": false, "message": "Unsupported direction"}))).into_response();
    };
    if !dir.is_dir() {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"verified": false, "message": "Model is not installed"}))).into_response();
    }
    let result = tokio::task::spawn_blocking(move || check_receipt(&dir)).await;
    match result {
        Ok(Ok(true)) => Json(serde_json::json!({"verified": true, "message": "All installed files passed SHA-256 verification."})).into_response(),
        Ok(Ok(false)) => (StatusCode::CONFLICT, Json(serde_json::json!({"verified": false, "message": "Required model files are missing."}))).into_response(),
        Ok(Err(error)) => (StatusCode::CONFLICT, Json(serde_json::json!({"verified": false, "message": error.to_string()}))).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"verified": false, "message": "Verification failed."}))).into_response(),
    }
}

pub(crate) async fn delete_model(
    headers: HeaderMap,
    State(state): State<Arc<ApiState>>,
    Json(request): Json<ModelDirection>,
) -> impl IntoResponse {
    if !trusted_ui_origin(&headers) { return StatusCode::FORBIDDEN.into_response(); }
    let Some(dir) = model_path(&state.models_directory, &request.direction) else {
        return (StatusCode::BAD_REQUEST, "Unsupported OPUS-MT direction").into_response();
    };
    if !dir.exists() { return (StatusCode::NOT_FOUND, "Model is not installed").into_response(); }
    let repo = model_source(&request.direction).expect("known direction").1;
    if state.downloads.read().await.values().any(|job|
        job.repo == repo && matches!(job.status.as_str(), "queued" | "downloading" | "cancelling")) {
        return (StatusCode::CONFLICT, "Cancel the active download before deleting this model").into_response();
    }
    // Serialized with translation requests; dropping the process releases model files.
    let mut worker = state.translation_worker.lock().await;
    crate::translation_api::stop_worker(&mut worker).await;
    let root = match state.models_directory.canonicalize() {
        Ok(root) => root,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Models root is unavailable").into_response(),
    };
    let canonical = match dir.canonicalize() {
        Ok(path) => path,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid model path").into_response(),
    };
    if !canonical.starts_with(root.join("opus-mt")) {
        return (StatusCode::BAD_REQUEST, "Model directory escapes models root").into_response();
    }
    match tokio::fs::remove_dir_all(&dir).await {
        Ok(()) => Json(serde_json::json!({"deleted": true, "direction": request.direction})).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_browser_origins_cannot_manage_model_files() {
        let mut headers = HeaderMap::new();
        assert!(trusted_ui_origin(&headers));
        headers.insert(ORIGIN, "https://example.com".parse().unwrap());
        assert!(!trusted_ui_origin(&headers));
        headers.insert(ORIGIN, "tauri://localhost".parse().unwrap());
        assert!(trusted_ui_origin(&headers));
    }
    #[test]
    fn only_known_directions_resolve_to_model_paths() {
        let root = Path::new("/models");
        assert_eq!(model_path(root, "en-zh"), Some(root.join("opus-mt/en-zh")));
        assert_eq!(model_path(root, "zh-en"), Some(root.join("opus-mt/zh-en")));
        assert!(model_path(root, "../../other").is_none());
    }
    #[test]
    fn sha256_receipt_detects_corruption() {
        let base = std::env::temp_dir().join(format!("opus-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        let mut files = Vec::new();
        for name in REQUIRED_FILES {
            std::fs::write(base.join(name), b"test").unwrap();
            let (sha256, size_bytes) = digest_file(&base.join(name)).unwrap();
            files.push(InstalledFile { filename: (*name).into(), sha256, size_bytes });
        }
        std::fs::write(base.join(RECEIPT), serde_json::to_vec(&Receipt {
            repository: "test".into(), revision: "test".into(), files
        }).unwrap()).unwrap();
        assert!(check_receipt(&base).unwrap());
        std::fs::write(base.join("model.bin"), b"corrupt").unwrap();
        assert!(check_receipt(&base).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }
}
