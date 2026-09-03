use axum::{
    Json, Router,
    extract::{Query, State},
    http::HeaderMap,
    response::{
        IntoResponse,
        sse::{Event, Sse},
    },
    routing::{get, post},
};
use chrono::Utc;
use deeplocal_core::{
    ChatMessage, ChatRole, GenerationParameters, GenerationRequest, LoadOptions, ModelDescriptor,
};
use deeplocal_runtime::RuntimeManager;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    env,
    path::{Component, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};
use tokio::{io::AsyncWriteExt, sync::RwLock};
use tower_http::cors::CorsLayer;
use uuid::Uuid;

const BLOCKED_MODEL_KEYWORDS: &[&str] = &[
    "nsfw",
    "uncensored",
    "abliterated",
    "abliteration",
    "dolphin",
    "erotic",
    "porn",
    "sex",
    "roleplay",
    "qwen",
    "qwq",
    "kimi",
    "moonshot",
    "deepseek",
    "deepseek-ai",
    "baichuan",
    "yi-",
    "01-ai",
    "internlm",
    "chatglm",
    "glm",
    "zhipu",
    "hunyuan",
    "tencent",
    "alibaba",
    "alipay",
    "bytedance",
    "doubao",
    "minimax",
    "stepfun",
    "baidu",
    "ernie",
    "sparkdesk",
    "iflytek",
];

#[derive(Clone)]
pub struct ApiState {
    pub runtime: RuntimeManager,
    pub downloads: Arc<RwLock<HashMap<String, DownloadJob>>>,
}

pub fn router(runtime: RuntimeManager) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/runtime/hardware", get(hardware))
        .route("/runtime/models", get(models).post(register_model))
        .route("/runtime/models/loaded", get(loaded_models))
        .route("/runtime/models/load", post(load_model))
        .route("/runtime/models/unload", post(unload_model))
        .route("/runtime/huggingface/search", get(huggingface_search))
        .route(
            "/runtime/huggingface/auth-check",
            post(huggingface_auth_check),
        )
        .route("/runtime/huggingface/download", post(huggingface_download))
        .route("/runtime/downloads", get(downloads))
        .route("/runtime/downloads/cancel", post(cancel_download))
        .route("/runtime/models/directory", get(models_directory))
        .route(
            "/runtime/models/open-directory",
            post(open_models_directory),
        )
        .route("/runtime/models/reveal", post(reveal_model_path))
        .route("/v1/models", get(openai_models))
        .route("/v1/chat/completions", post(chat_completions))
        .layer(CorsLayer::permissive())
        .with_state(Arc::new(ApiState {
            runtime,
            downloads: Arc::new(RwLock::new(HashMap::new())),
        }))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "name": "deepLocal" }))
}

async fn hardware() -> Json<serde_json::Value> {
    Json(serde_json::json!(deeplocal_hardware::detect_hardware()))
}

async fn loaded_models(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!(state.runtime.list_loaded_models().await))
}

async fn models(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    let models: Vec<_> = state
        .runtime
        .list_models()
        .await
        .into_iter()
        .map(model_with_local_size)
        .collect();
    Json(serde_json::json!(models))
}

async fn register_model(
    State(state): State<Arc<ApiState>>,
    Json(model): Json<ModelDescriptor>,
) -> impl IntoResponse {
    let model = model_with_local_size(absolutize_model_paths(model));
    state.runtime.register_model(model.clone()).await;
    (axum::http::StatusCode::CREATED, Json(model)).into_response()
}

#[derive(Debug, Deserialize)]
pub struct LoadModelRequest {
    pub model_id: String,
    #[serde(default = "default_backend")]
    pub backend: String,
    pub context_length: Option<u32>,
    pub gpu_layers: Option<i32>,
}

fn default_backend() -> String {
    "mock".to_string()
}

async fn load_model(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<LoadModelRequest>,
) -> impl IntoResponse {
    match state
        .runtime
        .load_registered_model(
            &body.backend,
            &body.model_id,
            LoadOptions {
                context_length: body.context_length,
                gpu_layers: body.gpu_layers,
            },
        )
        .await
    {
        Ok(handle) => Json(handle).into_response(),
        Err(error) => (axum::http::StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct UnloadModelRequest {
    pub model_id: String,
}

async fn unload_model(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<UnloadModelRequest>,
) -> impl IntoResponse {
    match state.runtime.unload_model(&body.model_id).await {
        Ok(()) => Json(serde_json::json!({ "status": "unloaded", "model_id": body.model_id }))
            .into_response(),
        Err(error) => (axum::http::StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct HuggingFaceSearchQuery {
    pub query: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HuggingFaceModelResult {
    pub repo: String,
    pub downloads: Option<u64>,
    pub likes: Option<u64>,
    pub files: Vec<HuggingFaceFileResult>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HuggingFaceFileResult {
    pub filename: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct HubModel {
    #[serde(rename = "modelId")]
    model_id: String,
    downloads: Option<u64>,
    likes: Option<u64>,
    siblings: Option<Vec<HubSibling>>,
}

#[derive(Debug, Deserialize)]
struct HubSibling {
    #[serde(rename = "rfilename")]
    filename: String,
    size: Option<u64>,
}

async fn huggingface_search(
    headers: HeaderMap,
    Query(query): Query<HuggingFaceSearchQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(12).clamp(1, 25).to_string();
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                error.to_string(),
            )
                .into_response();
        }
    };
    let token = huggingface_token_from_headers(&headers);
    let response = apply_huggingface_auth(
        client.get("https://huggingface.co/api/models").query(&[
            ("search", query.query.as_str()),
            ("filter", "gguf"),
            ("limit", limit.as_str()),
            ("full", "true"),
        ]),
        token.as_deref(),
    )
    .send()
    .await;

    let response = match response {
        Ok(response) => response,
        Err(error) => {
            return (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response();
        }
    };

    let models = match response.json::<Vec<HubModel>>().await {
        Ok(models) => models,
        Err(error) => {
            return (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response();
        }
    };

    let mut results: Vec<_> = models
        .into_iter()
        .filter(|model| !is_blocked_model_text(&model.model_id))
        .filter_map(|model| {
            let files: Vec<_> = model
                .siblings
                .unwrap_or_default()
                .into_iter()
                .filter(|file| file.filename.to_ascii_lowercase().ends_with(".gguf"))
                .filter(|file| !is_blocked_model_text(&file.filename))
                .map(|file| HuggingFaceFileResult {
                    filename: file.filename,
                    size_bytes: file.size,
                })
                .collect();
            (!files.is_empty()).then_some(HuggingFaceModelResult {
                repo: model.model_id,
                downloads: model.downloads,
                likes: model.likes,
                files,
            })
        })
        .collect();

    for result in &mut results {
        let sizes = huggingface_file_sizes(&client, &result.repo, token.as_deref())
            .await
            .unwrap_or_default();
        for file in result.files.iter_mut() {
            file.size_bytes = file
                .size_bytes
                .filter(|size| *size > 0)
                .or_else(|| sizes.get(&file.filename).copied());
        }
    }

    Json(results).into_response()
}

fn is_blocked_model_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    BLOCKED_MODEL_KEYWORDS
        .iter()
        .any(|keyword| lower.contains(keyword))
}

#[derive(Debug, Deserialize)]
struct HubTreeFile {
    path: String,
    size: Option<u64>,
    lfs: Option<HubTreeLfs>,
}

#[derive(Debug, Deserialize)]
struct HubTreeLfs {
    size: Option<u64>,
}

async fn huggingface_file_sizes(
    client: &reqwest::Client,
    repo: &str,
    token: Option<&str>,
) -> anyhow::Result<HashMap<String, u64>> {
    let url = format!("https://huggingface.co/api/models/{repo}/tree/main");
    let files = apply_huggingface_auth(client.get(url), token)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<HubTreeFile>>()
        .await?;

    Ok(files
        .into_iter()
        .filter_map(|file| {
            let size = file
                .lfs
                .and_then(|lfs| lfs.size)
                .or(file.size)
                .filter(|size| *size > 0)?;
            Some((file.path, size))
        })
        .collect())
}

#[derive(Debug, Deserialize)]
pub struct HuggingFaceDownloadRequest {
    pub repo: String,
    pub filename: String,
    pub model_id: Option<String>,
    pub size_bytes: Option<u64>,
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HuggingFaceAuthCheckRequest {
    pub token: Option<String>,
    pub repo: Option<String>,
    pub filename: Option<String>,
}

async fn huggingface_auth_check(
    Json(body): Json<HuggingFaceAuthCheckRequest>,
) -> impl IntoResponse {
    let token = normalized_token(body.token).or_else(env_huggingface_token);
    let Some(token) = token else {
        return Json(serde_json::json!({
            "ok": false,
            "authenticated": false,
            "message": "No Hugging Face token was provided."
        }))
        .into_response();
    };

    let client = reqwest::Client::new();
    let whoami = apply_huggingface_auth(
        client.get("https://huggingface.co/api/whoami-v2"),
        Some(&token),
    )
    .send()
    .await;
    let whoami = match whoami {
        Ok(response) if response.status().is_success() => {
            response.json::<serde_json::Value>().await.ok()
        }
        Ok(response) => {
            return Json(serde_json::json!({
                "ok": false,
                "authenticated": false,
                "message": format!("Token rejected by Hugging Face: {}", response.status())
            }))
            .into_response();
        }
        Err(error) => {
            return Json(serde_json::json!({
                "ok": false,
                "authenticated": false,
                "message": error.to_string()
            }))
            .into_response();
        }
    };

    if let (Some(repo), Some(filename)) = (body.repo, body.filename) {
        let response = apply_huggingface_auth(
            client.get(huggingface_resolve_url(&repo, &filename)),
            Some(&token),
        )
        .send()
        .await;
        return match response {
            Ok(response) if response.status().is_success() => Json(serde_json::json!({
                "ok": true,
                "authenticated": true,
                "repository_access": true,
                "user": whoami,
                "message": "Token can access this file."
            }))
            .into_response(),
            Ok(response) => Json(serde_json::json!({
                "ok": false,
                "authenticated": true,
                "repository_access": false,
                "user": whoami,
                "message": gated_repo_message(response.status())
            }))
            .into_response(),
            Err(error) => Json(serde_json::json!({
                "ok": false,
                "authenticated": true,
                "repository_access": false,
                "user": whoami,
                "message": error.to_string()
            }))
            .into_response(),
        };
    }

    Json(serde_json::json!({
        "ok": true,
        "authenticated": true,
        "user": whoami,
        "message": "Token is valid."
    }))
    .into_response()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadJob {
    pub id: String,
    pub repo: String,
    pub filename: String,
    pub status: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub local_path: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub cancel_requested: bool,
}

async fn downloads(State(state): State<Arc<ApiState>>) -> Json<Vec<DownloadJob>> {
    let mut downloads: Vec<_> = state.downloads.read().await.values().cloned().collect();
    downloads.sort_by(|a, b| a.id.cmp(&b.id));
    Json(downloads)
}

#[derive(Debug, Deserialize)]
pub struct CancelDownloadRequest {
    pub job_id: Option<String>,
    pub repo: Option<String>,
    pub filename: Option<String>,
}

async fn cancel_download(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<CancelDownloadRequest>,
) -> impl IntoResponse {
    let mut jobs = state.downloads.write().await;
    let job = if let Some(job_id) = body.job_id.as_deref() {
        jobs.get_mut(job_id)
    } else if let (Some(repo), Some(filename)) = (body.repo.as_deref(), body.filename.as_deref()) {
        jobs.values_mut()
            .find(|job| job.repo == repo && job.filename == filename && is_cancellable(&job.status))
    } else {
        None
    };

    match job {
        Some(job) if is_cancellable(&job.status) => {
            job.cancel_requested = true;
            job.status = "cancelling".to_string();
            Json(job.clone()).into_response()
        }
        Some(job) => (
            axum::http::StatusCode::BAD_REQUEST,
            format!("download is not cancellable: {}", job.status),
        )
            .into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "download job not found").into_response(),
    }
}

async fn models_directory() -> Json<serde_json::Value> {
    let path = models_root();
    Json(serde_json::json!({ "path": path.to_string_lossy() }))
}

async fn open_models_directory() -> impl IntoResponse {
    let path = models_root();
    if let Err(error) = std::fs::create_dir_all(&path) {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        )
            .into_response();
    }

    let result = if cfg!(target_os = "macos") {
        Command::new("open").arg(&path).status()
    } else if cfg!(target_os = "windows") {
        Command::new("explorer").arg(&path).status()
    } else {
        Command::new("xdg-open").arg(&path).status()
    };

    match result {
        Ok(status) if status.success() => {
            Json(serde_json::json!({ "status": "opened", "path": path.to_string_lossy() }))
                .into_response()
        }
        Ok(status) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("open command exited with status {status}"),
        )
            .into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct RevealModelPathRequest {
    pub path: String,
}

async fn reveal_model_path(Json(body): Json<RevealModelPathRequest>) -> impl IntoResponse {
    let path = absolute_path(PathBuf::from(body.path));
    let target = if path.exists() {
        path
    } else {
        path.parent()
            .map(PathBuf::from)
            .unwrap_or_else(models_root)
    };

    let result = if cfg!(target_os = "macos") && target.is_file() {
        Command::new("open").arg("-R").arg(&target).status()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(&target).status()
    } else if cfg!(target_os = "windows") && target.is_file() {
        Command::new("explorer")
            .arg(format!("/select,{}", target.to_string_lossy()))
            .status()
    } else if cfg!(target_os = "windows") {
        Command::new("explorer").arg(&target).status()
    } else {
        Command::new("xdg-open")
            .arg(target.parent().unwrap_or(&target))
            .status()
    };

    match result {
        Ok(status) if status.success() => {
            Json(serde_json::json!({ "status": "opened", "path": target.to_string_lossy() }))
                .into_response()
        }
        Ok(status) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("open command exited with status {status}"),
        )
            .into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        )
            .into_response(),
    }
}

async fn huggingface_download(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<HuggingFaceDownloadRequest>,
) -> impl IntoResponse {
    let job_id = Uuid::new_v4().to_string();
    let safe_repo = body.repo.replace('/', "__");
    let local_dir = models_root().join(safe_repo);
    let local_path = local_dir.join(&body.filename);
    let token = normalized_token(body.token.clone()).or_else(env_huggingface_token);
    let job = DownloadJob {
        id: job_id.clone(),
        repo: body.repo.clone(),
        filename: body.filename.clone(),
        status: "queued".to_string(),
        downloaded_bytes: 0,
        total_bytes: body.size_bytes,
        local_path: Some(local_path.to_string_lossy().to_string()),
        error: None,
        cancel_requested: false,
    };
    state
        .downloads
        .write()
        .await
        .insert(job_id.clone(), job.clone());

    let downloads = state.downloads.clone();
    let runtime = state.runtime.clone();
    tokio::spawn(async move {
        if let Err(error) = download_huggingface_file(
            downloads.clone(),
            runtime,
            job_id.clone(),
            HuggingFaceDownloadRequest { token, ..body },
            local_dir,
            local_path,
        )
        .await
        {
            let mut jobs = downloads.write().await;
            if let Some(job) = jobs.get_mut(&job_id) {
                job.status = "error".to_string();
                job.error = Some(error.to_string());
            }
        }
    });

    (axum::http::StatusCode::ACCEPTED, Json(job)).into_response()
}

fn models_root() -> PathBuf {
    absolute_path(PathBuf::from("./models"))
}

fn absolutize_model_paths(mut model: ModelDescriptor) -> ModelDescriptor {
    if let Some(path) = model.local_path.as_deref() {
        model.local_path = Some(absolute_path_string(path));
    }
    for file in &mut model.files {
        if let Some(path) = file.path.as_deref() {
            file.path = Some(absolute_path_string(path));
        }
    }
    model
}

fn model_with_local_size(mut model: ModelDescriptor) -> ModelDescriptor {
    for file in &mut model.files {
        if file.size_bytes.is_none() {
            file.size_bytes = file
                .path
                .as_deref()
                .and_then(|path| std::fs::metadata(path).ok())
                .map(|metadata| metadata.len());
        }
    }

    if model.size_bytes.is_none() {
        model.size_bytes = model
            .local_path
            .as_deref()
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|metadata| metadata.len())
            .or_else(|| model.files.iter().find_map(|file| file.size_bytes));
    }

    model
}

fn absolute_path_string(path: &str) -> String {
    absolute_path(PathBuf::from(path))
        .to_string_lossy()
        .to_string()
}

fn absolute_path(path: PathBuf) -> PathBuf {
    let absolute = if path.is_absolute() {
        path
    } else {
        env::current_dir()
        .map(|current| current.join(&path))
        .unwrap_or(path)
    };
    normalize_path(absolute)
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn huggingface_resolve_url(repo: &str, filename: &str) -> String {
    format!(
        "https://huggingface.co/{repo}/resolve/main/{}",
        filename
            .split('/')
            .map(urlencoding::encode)
            .collect::<Vec<_>>()
            .join("/")
    )
}

fn apply_huggingface_auth(
    builder: reqwest::RequestBuilder,
    token: Option<&str>,
) -> reqwest::RequestBuilder {
    match token.and_then(|token| (!token.trim().is_empty()).then_some(token.trim())) {
        Some(token) => builder.bearer_auth(token),
        None => builder,
    }
}

fn huggingface_token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-huggingface-token")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| normalized_token(Some(value.to_string())))
        .or_else(env_huggingface_token)
}

fn env_huggingface_token() -> Option<String> {
    env::var("HF_TOKEN")
        .ok()
        .or_else(|| env::var("HUGGINGFACE_TOKEN").ok())
        .and_then(|value| normalized_token(Some(value)))
}

fn normalized_token(token: Option<String>) -> Option<String> {
    token.and_then(|raw| {
        let trimmed = raw.trim().trim_matches('"').trim_matches('\'').trim();
        let token = trimmed
            .strip_prefix("Bearer ")
            .or_else(|| trimmed.strip_prefix("bearer "))
            .unwrap_or(trimmed)
            .trim()
            .to_string();
        (!token.is_empty()).then_some(token)
    })
}

fn gated_repo_message(status: reqwest::StatusCode) -> String {
    format!(
        "Hugging Face returned {status}. For official Google Gemma repositories, make sure you are logged in on Hugging Face, accepted the Gemma license for this exact repository, and are using a token with read access to public gated repositories."
    )
}

async fn download_huggingface_file(
    downloads: Arc<RwLock<HashMap<String, DownloadJob>>>,
    runtime: RuntimeManager,
    job_id: String,
    body: HuggingFaceDownloadRequest,
    local_dir: PathBuf,
    local_path: PathBuf,
) -> anyhow::Result<()> {
    {
        let mut jobs = downloads.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.status = "downloading".to_string();
        }
    }
    if download_cancel_requested(&downloads, &job_id).await {
        mark_download_cancelled(&downloads, &job_id, &local_path).await;
        return Ok(());
    }

    tokio::fs::create_dir_all(&local_dir).await?;
    let url = huggingface_resolve_url(&body.repo, &body.filename);
    let client = reqwest::Client::new();
    let response = apply_huggingface_auth(client.get(url), body.token.as_deref())
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        anyhow::bail!("{}", gated_repo_message(response.status()));
    }
    let response = response.error_for_status()?;
    let total = response.content_length().or(body.size_bytes);
    if download_cancel_requested(&downloads, &job_id).await {
        mark_download_cancelled(&downloads, &job_id, &local_path).await;
        return Ok(());
    }
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&local_path).await?;
    let mut downloaded = 0_u64;

    while let Some(chunk) = stream.next().await {
        if download_cancel_requested(&downloads, &job_id).await {
            drop(file);
            mark_download_cancelled(&downloads, &job_id, &local_path).await;
            return Ok(());
        }
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        let mut jobs = downloads.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.downloaded_bytes = downloaded;
            job.total_bytes = total;
        }
    }
    file.flush().await?;

    let model_id = body
        .model_id
        .unwrap_or_else(|| format!("{}:{}", body.repo, body.filename));
    let mut descriptor =
        ModelDescriptor::local_gguf(model_id, local_path.to_string_lossy().to_string());
    descriptor.source = "huggingface".to_string();
    descriptor.repo = Some(body.repo.clone());
    descriptor.size_bytes = total;
    runtime.register_model(descriptor).await;

    let mut jobs = downloads.write().await;
    if let Some(job) = jobs.get_mut(&job_id) {
        job.status = "downloaded".to_string();
        job.downloaded_bytes = downloaded;
        job.total_bytes = total;
    }
    Ok(())
}

fn is_cancellable(status: &str) -> bool {
    matches!(status, "queued" | "starting" | "downloading" | "cancelling")
}

async fn download_cancel_requested(
    downloads: &Arc<RwLock<HashMap<String, DownloadJob>>>,
    job_id: &str,
) -> bool {
    downloads
        .read()
        .await
        .get(job_id)
        .is_some_and(|job| job.cancel_requested)
}

async fn mark_download_cancelled(
    downloads: &Arc<RwLock<HashMap<String, DownloadJob>>>,
    job_id: &str,
    local_path: &PathBuf,
) {
    let _ = tokio::fs::remove_file(local_path).await;
    let mut jobs = downloads.write().await;
    if let Some(job) = jobs.get_mut(job_id) {
        job.status = "cancelled".to_string();
        job.error = None;
    }
}

async fn openai_models(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    let data: Vec<_> = state
        .runtime
        .list_loaded_models()
        .await
        .into_iter()
        .map(|model| {
            serde_json::json!({
                "id": model.id,
                "object": "model",
                "created": Utc::now().timestamp(),
                "owned_by": "deeplocal"
            })
        })
        .collect();
    Json(serde_json::json!({ "object": "list", "data": data }))
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChatRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    #[serde(default)]
    pub stream: bool,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub seed: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenAiMessage {
    pub role: String,
    pub content: String,
}

async fn chat_completions(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<OpenAiChatRequest>,
) -> impl IntoResponse {
    let request = GenerationRequest {
        model: body.model.clone(),
        messages: body
            .messages
            .into_iter()
            .map(|message| ChatMessage {
                id: Uuid::new_v4(),
                role: match message.role.as_str() {
                    "system" => ChatRole::System,
                    "assistant" => ChatRole::Assistant,
                    "tool" => ChatRole::Tool,
                    _ => ChatRole::User,
                },
                content: message.content,
                created_at: Utc::now(),
            })
            .collect(),
        parameters: GenerationParameters {
            temperature: body.temperature.unwrap_or(0.7),
            top_p: body.top_p.unwrap_or(0.95),
            max_tokens: body.max_tokens,
            seed: body.seed,
        },
        stream: body.stream,
    };

    if body.stream {
        let stream = match state.runtime.generate(request).await {
            Ok(stream) => stream,
            Err(error) => {
                let once = tokio_stream::once(Ok::<_, Infallible>(
                    Event::default()
                        .data(serde_json::json!({ "error": error.to_string() }).to_string()),
                ));
                return Sse::new(once)
                    .keep_alive(
                        axum::response::sse::KeepAlive::new().interval(Duration::from_secs(15)),
                    )
                    .into_response();
            }
        };
        let sse = stream.map(move |token| {
            let token = token
                .map_err(|error| error.to_string())
                .unwrap_or_else(|error| deeplocal_core::GeneratedToken {
                    text: error,
                    index: 0,
                    done: true,
                });
            if token.done {
                Ok::<_, Infallible>(Event::default().data("[DONE]"))
            } else {
                Ok(Event::default().data(
                    serde_json::json!({
                        "id": format!("chatcmpl-{}", Uuid::new_v4()),
                        "object": "chat.completion.chunk",
                        "created": Utc::now().timestamp(),
                        "model": body.model,
                        "choices": [{
                            "index": 0,
                            "delta": { "content": token.text },
                            "finish_reason": null
                        }]
                    })
                    .to_string(),
                ))
            }
        });
        return Sse::new(sse)
            .keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(15)))
            .into_response();
    }

    let mut text = String::new();
    match state.runtime.generate(request).await {
        Ok(mut stream) => {
            while let Some(token) = stream.next().await {
                match token {
                    Ok(token) => text.push_str(&token.text),
                    Err(error) => {
                        return (
                            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                            error.to_string(),
                        )
                            .into_response();
                    }
                }
            }
        }
        Err(error) => {
            return (axum::http::StatusCode::BAD_REQUEST, error.to_string()).into_response();
        }
    }

    Json(serde_json::json!({
        "id": format!("chatcmpl-{}", Uuid::new_v4()),
        "object": "chat.completion",
        "created": Utc::now().timestamp(),
        "model": body.model,
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": text },
            "finish_reason": "stop"
        }]
    }))
    .into_response()
}
