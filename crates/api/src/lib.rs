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
    ChatMessage, ChatRole, DownloadJob, GenerationParameters, GenerationRequest, LoadOptions,
    LoadedModelStatus, ModelDescriptor, ModelHandle, SearchFiltersConfig,
};
use deeplocal_runtime::RuntimeManager;
use deeplocal_storage::Storage;
use futures::{StreamExt, stream::FuturesUnordered};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    env,
    path::{Component, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    sync::{Mutex, RwLock},
};
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[derive(Clone)]
pub struct ApiState {
    pub runtime: RuntimeManager,
    pub downloads: Arc<RwLock<HashMap<String, DownloadJob>>>,
    pub storage: Arc<Mutex<Storage>>,
    pub search_filters: Arc<RwLock<SearchFiltersConfig>>,
    pub huggingface_size_cache: Arc<RwLock<HashMap<String, HashMap<String, u64>>>>,
}

pub fn router(runtime: RuntimeManager) -> Router {
    router_with_cors(runtime, true)
}

pub fn router_with_cors(runtime: RuntimeManager, enable_cors: bool) -> Router {
    router_with_options(runtime, enable_cors, SearchFiltersConfig::default())
}

pub fn router_with_options(
    runtime: RuntimeManager,
    enable_cors: bool,
    initial_search_filters: SearchFiltersConfig,
) -> Router {
    let storage = open_default_storage();
    let restored_downloads = restore_download_jobs(&storage);
    let router = Router::new()
        .route("/health", get(health))
        .route("/runtime/hardware", get(hardware))
        .route("/runtime/backends", get(backends))
        .route("/runtime/models", get(models).post(register_model))
        .route("/runtime/models/delete", post(delete_model))
        .route("/runtime/models/loaded", get(loaded_models))
        .route("/runtime/models/load", post(load_model))
        .route("/runtime/models/rescan", post(rescan_models))
        .route("/runtime/models/unload", post(unload_model))
        .route("/runtime/huggingface/search", get(huggingface_search))
        .route("/runtime/search-filters", get(get_search_filters))
        .route(
            "/runtime/search-filters/blocked-keywords",
            post(add_blocked_keyword),
        )
        .route(
            "/runtime/huggingface/auth-check",
            post(huggingface_auth_check),
        )
        .route("/runtime/huggingface/download", post(huggingface_download))
        .route("/runtime/downloads", get(downloads))
        .route(
            "/runtime/downloads/clear-history",
            post(clear_download_history),
        )
        .route("/runtime/downloads/cancel", post(cancel_download))
        .route("/runtime/downloads/discard", post(discard_download))
        .route(
            "/runtime/chat/conversations",
            get(chat_conversations).post(create_chat_conversation),
        )
        .route(
            "/runtime/chat/conversations/rename",
            post(rename_chat_conversation),
        )
        .route(
            "/runtime/chat/conversations/delete",
            post(delete_chat_conversation),
        )
        .route(
            "/runtime/chat/conversations/model",
            post(update_chat_conversation_model),
        )
        .route("/runtime/chat/messages", post(append_chat_message))
        .route("/runtime/models/directory", get(models_directory))
        .route(
            "/runtime/models/open-directory",
            post(open_models_directory),
        )
        .route("/runtime/models/reveal", post(reveal_model_path))
        .route("/v1/models", get(openai_models))
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(Arc::new(ApiState {
            runtime,
            downloads: Arc::new(RwLock::new(restored_downloads)),
            storage: Arc::new(Mutex::new(storage)),
            search_filters: Arc::new(RwLock::new(initial_search_filters)),
            huggingface_size_cache: Arc::new(RwLock::new(HashMap::new())),
        }));

    if enable_cors {
        router.layer(CorsLayer::permissive())
    } else {
        router
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "name": "deepLocal" }))
}

fn open_default_storage() -> Storage {
    Storage::open("deeplocal.sqlite3")
        .or_else(|_| Storage::open_memory())
        .expect("open chat storage")
}

fn restore_download_jobs(storage: &Storage) -> HashMap<String, DownloadJob> {
    storage
        .list_recent_download_jobs(100)
        .unwrap_or_default()
        .into_iter()
        .map(|mut job| {
            if is_cancellable(&job.status) {
                job.status = "error".to_string();
                job.error = Some("Download interrupted by application restart.".to_string());
                job.cancel_requested = false;
                job.updated_at = Utc::now();
                let _ = storage.upsert_download_job(&job);
            }
            (job.id.clone(), job)
        })
        .collect()
}

async fn hardware() -> Json<serde_json::Value> {
    Json(serde_json::json!(deeplocal_hardware::detect_hardware()))
}

async fn loaded_models(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!(state.runtime.list_loaded_models().await))
}

async fn backends(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!(
        state.runtime.list_backend_statuses().await
    ))
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
    if state.runtime.get_model(&model.id).await.is_some() {
        return (
            axum::http::StatusCode::CONFLICT,
            format!("Model id already exists: {}", model.id),
        )
            .into_response();
    }
    let model = model_with_local_size(absolutize_model_paths(model));
    state.runtime.register_model(model.clone()).await;
    (axum::http::StatusCode::CREATED, Json(model)).into_response()
}

#[derive(Debug, Serialize)]
struct DiscoveredModelFile {
    filename: String,
    path: String,
    size_bytes: u64,
    suggested_model_id: String,
}

async fn rescan_models(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let registered_models = state.runtime.list_models().await;
    let registered_paths: std::collections::HashSet<_> = registered_models
        .iter()
        .filter_map(|model| model.local_path.as_deref())
        .map(absolute_path_string)
        .collect();
    let mut used_ids: std::collections::HashSet<_> = registered_models
        .iter()
        .map(|model| model.id.clone())
        .collect();
    let root = models_root();
    let mut discovered = Vec::new();

    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Json(serde_json::json!({ "files": discovered })).into_response();
        }
        Err(error) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                error.to_string(),
            )
                .into_response();
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_gguf_path(&path) {
            continue;
        }
        let absolute_path = absolute_path(path);
        let path_string = absolute_path.to_string_lossy().to_string();
        if registered_paths.contains(&path_string) {
            continue;
        }
        let Ok(metadata) = std::fs::metadata(&absolute_path) else {
            continue;
        };
        let filename = absolute_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "model.gguf".to_string());
        let base_id = model_id_from_filename(&filename);
        let suggested_model_id = unique_model_id(&base_id, &mut used_ids);
        discovered.push(DiscoveredModelFile {
            filename,
            path: path_string,
            size_bytes: metadata.len(),
            suggested_model_id,
        });
    }

    discovered.sort_by(|a, b| a.filename.cmp(&b.filename));
    Json(serde_json::json!({ "files": discovered })).into_response()
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
pub struct DeleteModelRequest {
    pub model_id: String,
    #[serde(default)]
    pub delete_file: bool,
}

async fn delete_model(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<DeleteModelRequest>,
) -> impl IntoResponse {
    if state.runtime.is_model_loaded(&body.model_id).await {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "Unload the model before deleting it.",
        )
            .into_response();
    }

    let Some(model) = state.runtime.get_model(&body.model_id).await else {
        return (axum::http::StatusCode::NOT_FOUND, "model not found").into_response();
    };

    if body.delete_file {
        match model.local_path.as_deref() {
            Some(path) if is_inside_models_root(path) => {}
            Some(_) => {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    "Refusing to delete a file outside the models directory.",
                )
                    .into_response();
            }
            None => {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    "No local file is registered for this model.",
                )
                    .into_response();
            }
        }
    }

    let Some(model) = state.runtime.remove_model(&body.model_id).await else {
        return (axum::http::StatusCode::NOT_FOUND, "model not found").into_response();
    };

    let mut deleted_file = false;
    if body.delete_file {
        if let Some(path) = model.local_path.as_deref() {
            match std::fs::remove_file(path) {
                Ok(()) => deleted_file = true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
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

    Json(serde_json::json!({
        "status": "deleted",
        "model_id": body.model_id,
        "deleted_file": deleted_file
    }))
    .into_response()
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
    State(state): State<Arc<ApiState>>,
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
    let blocked_keywords = state.search_filters.read().await.blocked_keywords.clone();

    let mut results: Vec<_> = models
        .into_iter()
        .filter(|model| !is_blocked_model_text(&model.model_id, &blocked_keywords))
        .filter_map(|model| {
            let files: Vec<_> = model
                .siblings
                .unwrap_or_default()
                .into_iter()
                .filter(|file| file.filename.to_ascii_lowercase().ends_with(".gguf"))
                .filter(|file| !is_blocked_model_text(&file.filename, &blocked_keywords))
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

    fill_missing_huggingface_sizes(
        &client,
        token.as_deref(),
        &state.huggingface_size_cache,
        &mut results,
    )
    .await;

    Json(results).into_response()
}

async fn fill_missing_huggingface_sizes(
    client: &reqwest::Client,
    token: Option<&str>,
    cache: &Arc<RwLock<HashMap<String, HashMap<String, u64>>>>,
    results: &mut [HuggingFaceModelResult],
) {
    let repos_needing_sizes: Vec<_> = results
        .iter()
        .filter(|result| {
            result
                .files
                .iter()
                .any(|file| !has_known_size(file.size_bytes))
        })
        .map(|result| result.repo.clone())
        .collect();
    if repos_needing_sizes.is_empty() {
        return;
    }

    let cached_sizes = cache.read().await.clone();
    let mut missing_repos = Vec::new();
    for result in results.iter_mut() {
        if let Some(sizes) = cached_sizes.get(&result.repo) {
            apply_huggingface_sizes(result, sizes);
        } else if repos_needing_sizes.contains(&result.repo) {
            missing_repos.push(result.repo.clone());
        }
    }
    missing_repos.sort();
    missing_repos.dedup();

    let mut lookups = FuturesUnordered::new();
    for repo in missing_repos {
        let client = client.clone();
        let token = token.map(str::to_string);
        lookups.push(async move {
            let sizes = tokio::time::timeout(
                Duration::from_secs(3),
                huggingface_file_sizes(&client, &repo, token.as_deref()),
            )
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
            (repo, sizes)
        });
    }

    let mut fetched_sizes = HashMap::new();
    while let Some((repo, sizes)) = lookups.next().await {
        if !sizes.is_empty() {
            fetched_sizes.insert(repo, sizes);
        }
    }
    if fetched_sizes.is_empty() {
        return;
    }

    cache.write().await.extend(fetched_sizes.clone());
    for result in results {
        if let Some(sizes) = fetched_sizes.get(&result.repo) {
            apply_huggingface_sizes(result, sizes);
        }
    }
}

fn has_known_size(size: Option<u64>) -> bool {
    size.is_some_and(|size| size > 0)
}

fn apply_huggingface_sizes(result: &mut HuggingFaceModelResult, sizes: &HashMap<String, u64>) {
    for file in result.files.iter_mut() {
        if !has_known_size(file.size_bytes) {
            file.size_bytes = sizes.get(&file.filename).copied();
        }
    }
}

fn is_blocked_model_text(text: &str, blocked_keywords: &[String]) -> bool {
    let lower = text.to_ascii_lowercase();
    blocked_keywords
        .iter()
        .map(|keyword| keyword.trim().to_ascii_lowercase())
        .filter(|keyword| !keyword.is_empty())
        .any(|keyword| lower.contains(&keyword))
}

async fn get_search_filters(State(state): State<Arc<ApiState>>) -> Json<SearchFiltersConfig> {
    Json(state.search_filters.read().await.clone())
}

#[derive(Debug, Deserialize)]
struct AddBlockedKeywordRequest {
    keyword: String,
}

async fn add_blocked_keyword(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<AddBlockedKeywordRequest>,
) -> impl IntoResponse {
    let keyword = body.keyword.trim().to_ascii_lowercase();
    if keyword.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, "keyword is required").into_response();
    }

    let mut filters = state.search_filters.write().await;
    if !filters
        .blocked_keywords
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&keyword))
    {
        filters.blocked_keywords.push(keyword);
        filters.blocked_keywords.sort();
    }

    Json(filters.clone()).into_response()
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

#[cfg(test)]
mod tests {
    use super::{
        HuggingFaceFileResult, HuggingFaceModelResult, OpenAiChatRequest, apply_huggingface_sizes,
        calculate_eta_seconds, is_download_history, is_gguf_header, is_inside_models_root,
        model_id_from_filename, openai_model_data, range_header, unique_model_id,
    };
    use deeplocal_core::{LoadedModelStatus, ModelDescriptor, ModelHandle};
    use std::collections::{HashMap, HashSet};

    #[test]
    fn eta_uses_remaining_bytes_and_speed() {
        assert_eq!(calculate_eta_seconds(40, Some(100), Some(20.0)), Some(3));
    }

    #[test]
    fn eta_is_unknown_without_total_size() {
        assert_eq!(calculate_eta_seconds(40, None, Some(20.0)), None);
    }

    #[test]
    fn eta_is_unknown_without_speed() {
        assert_eq!(calculate_eta_seconds(40, Some(100), None), None);
    }

    #[test]
    fn eta_is_zero_when_download_is_complete() {
        assert_eq!(calculate_eta_seconds(100, Some(100), Some(20.0)), Some(0));
    }

    #[test]
    fn history_statuses_are_terminal_download_jobs() {
        assert!(is_download_history("downloaded"));
        assert!(is_download_history("cancelled"));
        assert!(is_download_history("error"));
        assert!(!is_download_history("downloading"));
        assert!(!is_download_history("queued"));
    }

    #[test]
    fn range_header_is_omitted_for_new_downloads() {
        assert_eq!(range_header(0), None);
    }

    #[test]
    fn range_header_resumes_from_existing_bytes() {
        assert_eq!(range_header(42), Some("bytes=42-".to_string()));
    }

    #[test]
    fn gguf_header_is_validated() {
        assert!(is_gguf_header(b"GGUF"));
        assert!(!is_gguf_header(b"HTML"));
    }

    #[test]
    fn model_delete_paths_must_stay_inside_models_root() {
        assert!(is_inside_models_root("./models/example/model.gguf"));
        assert!(!is_inside_models_root("../outside/model.gguf"));
    }

    #[test]
    fn model_ids_are_derived_from_gguf_filenames() {
        assert_eq!(
            model_id_from_filename("gemma 3 1b.Q4_K_M.gguf"),
            "gemma-3-1b.Q4_K_M"
        );
        assert_eq!(model_id_from_filename("!!!.gguf"), "local-model");
    }

    #[test]
    fn discovered_model_ids_do_not_duplicate_existing_ids() {
        let mut used = HashSet::from(["gemma".to_string(), "gemma-2".to_string()]);
        assert_eq!(unique_model_id("gemma", &mut used), "gemma-3");
        assert!(used.contains("gemma-3"));
    }

    #[test]
    fn openai_models_include_available_and_loaded_models() {
        let available = vec![
            ModelDescriptor::local_gguf("available-model", "available.gguf"),
            ModelDescriptor::local_gguf("loaded-model", "loaded.gguf"),
        ];
        let loaded = vec![ModelHandle {
            id: "loaded-model".to_string(),
            backend: "llama.cpp".to_string(),
            status: LoadedModelStatus::Loaded,
        }];

        let data = openai_model_data(available, loaded);

        assert_eq!(data.len(), 2);
        assert_eq!(data[0]["id"], "available-model");
        assert_eq!(data[0]["object"], "model");
        assert_eq!(data[0]["owned_by"], "deeplocal");
        assert_eq!(data[0]["status"], "available");
        assert!(data[0]["created"].as_i64().is_some());
        assert_eq!(data[1]["id"], "loaded-model");
        assert_eq!(data[1]["status"], "loaded");
        assert_eq!(data[1]["backend"], "llama.cpp");
    }

    #[test]
    fn openai_models_keep_orphan_loaded_handles_visible() {
        let data = openai_model_data(
            Vec::new(),
            vec![ModelHandle {
                id: "orphan-loaded".to_string(),
                backend: "mock".to_string(),
                status: LoadedModelStatus::Loaded,
            }],
        );

        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["id"], "orphan-loaded");
        assert_eq!(data[0]["object"], "model");
        assert_eq!(data[0]["created"], 0);
        assert_eq!(data[0]["status"], "loaded");
    }

    #[test]
    fn openai_chat_request_parses_common_generation_parameters() {
        let request: OpenAiChatRequest = serde_json::from_value(serde_json::json!({
            "model": "local-model",
            "messages": [{ "role": "user", "content": "hello" }],
            "temperature": 0.2,
            "top_p": 0.8,
            "max_tokens": 128,
            "stop": ["END", "\nUser:"],
            "stream": true,
            "unsupported_client_field": "ignored"
        }))
        .expect("parse request");

        assert_eq!(request.model, "local-model");
        assert!(request.stream);
        assert_eq!(request.temperature, Some(0.2));
        assert_eq!(request.top_p, Some(0.8));
        assert_eq!(request.max_tokens, Some(128));
        assert_eq!(request.stop, vec!["END".to_string(), "\nUser:".to_string()]);
    }

    #[test]
    fn openai_chat_request_accepts_single_stop_sequence_and_defaults_stream() {
        let request: OpenAiChatRequest = serde_json::from_value(serde_json::json!({
            "model": "local-model",
            "messages": [{ "role": "user", "content": "hello" }],
            "stop": "END"
        }))
        .expect("parse request");

        assert!(!request.stream);
        assert_eq!(request.stop, vec!["END".to_string()]);
    }

    #[test]
    fn openai_chat_request_rejects_invalid_stop_sequences() {
        let error = serde_json::from_value::<OpenAiChatRequest>(serde_json::json!({
            "model": "local-model",
            "messages": [{ "role": "user", "content": "hello" }],
            "stop": [42]
        }))
        .expect_err("invalid stop should fail");

        assert!(
            error
                .to_string()
                .contains("stop must be a string or an array of strings")
        );
    }

    #[test]
    fn huggingface_size_fallback_keeps_sibling_metadata() {
        let mut result = HuggingFaceModelResult {
            repo: "owner/repo".to_string(),
            downloads: None,
            likes: None,
            files: vec![HuggingFaceFileResult {
                filename: "model.gguf".to_string(),
                size_bytes: Some(42),
            }],
        };
        let tree_sizes = HashMap::from([("model.gguf".to_string(), 100)]);

        apply_huggingface_sizes(&mut result, &tree_sizes);

        assert_eq!(result.files[0].size_bytes, Some(42));
    }

    #[test]
    fn huggingface_size_fallback_fills_missing_sizes() {
        let mut result = HuggingFaceModelResult {
            repo: "owner/repo".to_string(),
            downloads: None,
            likes: None,
            files: vec![HuggingFaceFileResult {
                filename: "model.gguf".to_string(),
                size_bytes: None,
            }],
        };
        let tree_sizes = HashMap::from([("model.gguf".to_string(), 100)]);

        apply_huggingface_sizes(&mut result, &tree_sizes);

        assert_eq!(result.files[0].size_bytes, Some(100));
    }
}

async fn downloads(State(state): State<Arc<ApiState>>) -> Json<Vec<DownloadJob>> {
    let mut downloads: Vec<_> = state.downloads.read().await.values().cloned().collect();
    downloads.sort_by(|a, b| a.id.cmp(&b.id));
    Json(downloads)
}

async fn clear_download_history(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    let mut downloads = state.downloads.write().await;
    let before = downloads.len();
    downloads.retain(|_, job| !is_download_history(&job.status));
    let cleared = before - downloads.len();
    drop(downloads);

    let storage = state.storage.lock().await;
    let _ = storage.delete_download_jobs_by_statuses(&["downloaded", "cancelled", "error"]);
    Json(serde_json::json!({ "cleared": cleared }))
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
            job.updated_at = Utc::now();
            let job = job.clone();
            drop(jobs);
            persist_download_job(&state.storage, &job).await;
            Json(job).into_response()
        }
        Some(job) => (
            axum::http::StatusCode::BAD_REQUEST,
            format!("download is not cancellable: {}", job.status),
        )
            .into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "download job not found").into_response(),
    }
}

async fn discard_download(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<CancelDownloadRequest>,
) -> impl IntoResponse {
    let mut jobs = state.downloads.write().await;
    let job_id = if let Some(job_id) = body.job_id.as_deref() {
        jobs.get(job_id).map(|job| job.id.clone())
    } else if let (Some(repo), Some(filename)) = (body.repo.as_deref(), body.filename.as_deref()) {
        jobs.values()
            .find(|job| job.repo == repo && job.filename == filename)
            .map(|job| job.id.clone())
    } else {
        None
    };

    let Some(job_id) = job_id else {
        return (axum::http::StatusCode::NOT_FOUND, "download job not found").into_response();
    };

    let Some(mut job) = jobs.remove(&job_id) else {
        return (axum::http::StatusCode::NOT_FOUND, "download job not found").into_response();
    };

    if is_cancellable(&job.status) {
        job.cancel_requested = true;
        job.status = "cancelling".to_string();
        job.updated_at = Utc::now();
    }
    drop(jobs);

    if let Some(local_path) = job.local_path.as_deref() {
        let partial_path = partial_download_path(&PathBuf::from(local_path));
        let _ = tokio::fs::remove_file(partial_path).await;
    }

    let storage = state.storage.lock().await;
    let _ = storage.delete_download_job(&job.id);

    Json(serde_json::json!({ "discarded": true, "id": job.id })).into_response()
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
        path.parent().map(PathBuf::from).unwrap_or_else(models_root)
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
    let now = Utc::now();
    let job = DownloadJob {
        id: job_id.clone(),
        repo: body.repo.clone(),
        filename: body.filename.clone(),
        status: "queued".to_string(),
        downloaded_bytes: 0,
        total_bytes: body.size_bytes,
        speed_bytes_per_sec: None,
        eta_seconds: None,
        local_path: Some(local_path.to_string_lossy().to_string()),
        error: None,
        cancel_requested: false,
        created_at: now,
        updated_at: now,
    };
    state
        .downloads
        .write()
        .await
        .insert(job_id.clone(), job.clone());
    persist_download_job(&state.storage, &job).await;

    let downloads = state.downloads.clone();
    let runtime = state.runtime.clone();
    let storage = state.storage.clone();
    tokio::spawn(async move {
        if let Err(error) = download_huggingface_file(
            downloads.clone(),
            storage.clone(),
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
                job.updated_at = Utc::now();
                let job = job.clone();
                drop(jobs);
                persist_download_job(&storage, &job).await;
            }
        }
    });

    (axum::http::StatusCode::ACCEPTED, Json(job)).into_response()
}

fn models_root() -> PathBuf {
    absolute_path(PathBuf::from("./models"))
}

fn is_gguf_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
}

fn model_id_from_filename(filename: &str) -> String {
    let stem = filename
        .strip_suffix(".gguf")
        .or_else(|| filename.strip_suffix(".GGUF"))
        .unwrap_or(filename);
    let id: String = stem
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let id = id.trim_matches('-').to_string();
    if id.is_empty() {
        "local-model".to_string()
    } else {
        id
    }
}

fn unique_model_id(base_id: &str, used_ids: &mut std::collections::HashSet<String>) -> String {
    if used_ids.insert(base_id.to_string()) {
        return base_id.to_string();
    }

    for index in 2.. {
        let candidate = format!("{base_id}-{index}");
        if used_ids.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("unbounded model id suffix search should always find a free id")
}

fn is_inside_models_root(path: &str) -> bool {
    let models_root = models_root();
    let path = absolute_path(PathBuf::from(path));
    path.starts_with(models_root)
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

fn partial_download_path(local_path: &PathBuf) -> PathBuf {
    let mut partial = local_path.as_os_str().to_os_string();
    partial.push(".partial");
    PathBuf::from(partial)
}

fn range_header(resume_from: u64) -> Option<String> {
    (resume_from > 0).then(|| format!("bytes={resume_from}-"))
}

async fn validate_downloaded_gguf(
    path: &PathBuf,
    expected_size: Option<u64>,
) -> anyhow::Result<()> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| anyhow::anyhow!("downloaded file is missing: {error}"))?;
    let actual_size = metadata.len();
    if actual_size == 0 {
        anyhow::bail!("downloaded file is empty");
    }
    if let Some(expected_size) = expected_size {
        if actual_size != expected_size {
            anyhow::bail!(
                "downloaded file size mismatch: expected {expected_size} bytes, got {actual_size}"
            );
        }
    }

    let mut file = tokio::fs::File::open(path).await?;
    let mut header = [0_u8; 4];
    file.read_exact(&mut header).await.map_err(|error| {
        anyhow::anyhow!("downloaded file is too small for a GGUF header: {error}")
    })?;
    if !is_gguf_header(&header) {
        anyhow::bail!("downloaded file is not a GGUF file");
    }
    Ok(())
}

fn is_gguf_header(header: &[u8; 4]) -> bool {
    header == b"GGUF"
}

async fn download_huggingface_file(
    downloads: Arc<RwLock<HashMap<String, DownloadJob>>>,
    storage: Arc<Mutex<Storage>>,
    runtime: RuntimeManager,
    job_id: String,
    body: HuggingFaceDownloadRequest,
    local_dir: PathBuf,
    local_path: PathBuf,
) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(&local_dir).await?;
    let partial_path = partial_download_path(&local_path);
    let resume_from = tokio::fs::metadata(&partial_path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    {
        let mut jobs = downloads.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.status = "downloading".to_string();
            job.downloaded_bytes = resume_from;
            job.updated_at = Utc::now();
            let job = job.clone();
            drop(jobs);
            persist_download_job(&storage, &job).await;
        }
    }
    if download_cancel_requested(&downloads, &job_id).await {
        mark_download_cancelled(&downloads, &storage, &job_id, &local_path).await;
        return Ok(());
    }

    let url = huggingface_resolve_url(&body.repo, &body.filename);
    let client = reqwest::Client::new();
    let mut request = apply_huggingface_auth(client.get(url), body.token.as_deref());
    if let Some(header) = range_header(resume_from) {
        request = request.header(reqwest::header::RANGE, header);
    }
    let response = request.send().await?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        anyhow::bail!("{}", gated_repo_message(response.status()));
    }
    let resumes_partial =
        resume_from > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let response = response.error_for_status()?;
    let downloaded_start = if resume_from > 0 && !resumes_partial {
        tokio::fs::remove_file(&partial_path).await.ok();
        0
    } else {
        resume_from
    };
    let total = response
        .content_length()
        .map(|length| length + downloaded_start)
        .or(body.size_bytes);
    if download_cancel_requested(&downloads, &job_id).await {
        mark_download_cancelled(&downloads, &storage, &job_id, &local_path).await;
        return Ok(());
    }
    let mut stream = response.bytes_stream();
    let file = if downloaded_start > 0 {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&partial_path)
            .await?
    } else {
        tokio::fs::File::create(&partial_path).await?
    };
    let mut file = BufWriter::new(file);
    let mut downloaded = downloaded_start;
    let mut last_sample_at = Instant::now();
    let mut last_sample_bytes = downloaded_start;

    while let Some(chunk) = stream.next().await {
        if download_cancel_requested(&downloads, &job_id).await {
            drop(file);
            mark_download_cancelled(&downloads, &storage, &job_id, &local_path).await;
            return Ok(());
        }
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        let now = Instant::now();
        let elapsed = now.duration_since(last_sample_at).as_secs_f64();
        let speed = (elapsed > 0.0)
            .then(|| (downloaded.saturating_sub(last_sample_bytes) as f64) / elapsed)
            .filter(|value| value.is_finite() && *value > 0.0);
        last_sample_at = now;
        last_sample_bytes = downloaded;
        let mut jobs = downloads.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.downloaded_bytes = downloaded;
            job.total_bytes = total;
            job.speed_bytes_per_sec = speed;
            job.eta_seconds = calculate_eta_seconds(downloaded, total, speed);
            job.updated_at = Utc::now();
            let job = job.clone();
            drop(jobs);
            persist_download_job(&storage, &job).await;
        }
    }
    file.flush().await?;
    drop(file);

    if let Some(expected_size) = total {
        if downloaded != expected_size {
            anyhow::bail!(
                "downloaded file size mismatch: expected {expected_size} bytes, got {downloaded}"
            );
        }
    }

    tokio::fs::rename(&partial_path, &local_path).await?;
    validate_downloaded_gguf(&local_path, total).await?;

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
        job.speed_bytes_per_sec = None;
        job.eta_seconds = Some(0);
        job.updated_at = Utc::now();
        let job = job.clone();
        drop(jobs);
        persist_download_job(&storage, &job).await;
    }
    Ok(())
}

fn calculate_eta_seconds(
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    speed_bytes_per_sec: Option<f64>,
) -> Option<u64> {
    let total = total_bytes?;
    let speed = speed_bytes_per_sec?;
    if total <= downloaded_bytes || speed <= 0.0 || !speed.is_finite() {
        return Some(0);
    }
    Some(((total - downloaded_bytes) as f64 / speed).ceil() as u64)
}

fn is_cancellable(status: &str) -> bool {
    matches!(status, "queued" | "starting" | "downloading" | "cancelling")
}

fn is_download_history(status: &str) -> bool {
    matches!(status, "downloaded" | "cancelled" | "error")
}

async fn download_cancel_requested(
    downloads: &Arc<RwLock<HashMap<String, DownloadJob>>>,
    job_id: &str,
) -> bool {
    downloads
        .read()
        .await
        .get(job_id)
        .map(|job| job.cancel_requested)
        .unwrap_or(true)
}

async fn mark_download_cancelled(
    downloads: &Arc<RwLock<HashMap<String, DownloadJob>>>,
    storage: &Arc<Mutex<Storage>>,
    job_id: &str,
    _local_path: &PathBuf,
) {
    let mut jobs = downloads.write().await;
    if let Some(job) = jobs.get_mut(job_id) {
        job.status = "cancelled".to_string();
        job.error = None;
        job.cancel_requested = false;
        job.updated_at = Utc::now();
        let job = job.clone();
        drop(jobs);
        persist_download_job(storage, &job).await;
    }
}

async fn persist_download_job(storage: &Arc<Mutex<Storage>>, job: &DownloadJob) {
    let storage = storage.lock().await;
    let _ = storage.upsert_download_job(job);
}

async fn openai_models(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    let available = state.runtime.list_models().await;
    let loaded = state.runtime.list_loaded_models().await;
    let data = openai_model_data(available, loaded);
    Json(serde_json::json!({ "object": "list", "data": data }))
}

fn openai_model_data(
    available: Vec<ModelDescriptor>,
    loaded: Vec<ModelHandle>,
) -> Vec<serde_json::Value> {
    let loaded_by_id: std::collections::HashMap<_, _> = loaded
        .into_iter()
        .map(|handle| (handle.id.clone(), handle))
        .collect();
    let mut seen_ids = std::collections::HashSet::new();
    let mut data = Vec::new();

    for model in available {
        let loaded_handle = loaded_by_id.get(&model.id);
        seen_ids.insert(model.id.clone());
        data.push(serde_json::json!({
            "id": model.id,
            "object": "model",
            "created": model.created_at.timestamp(),
            "owned_by": "deeplocal",
            "status": if loaded_handle.is_some() { "loaded" } else { "available" },
            "backend": loaded_handle.map(|handle| handle.backend.as_str()),
        }));
    }

    for handle in loaded_by_id.values() {
        if seen_ids.contains(&handle.id) {
            continue;
        }
        data.push(serde_json::json!({
            "id": handle.id,
            "object": "model",
            "created": 0,
            "owned_by": "deeplocal",
            "status": match handle.status {
                LoadedModelStatus::Loading => "loading",
                LoadedModelStatus::Loaded => "loaded",
                LoadedModelStatus::Unloading => "unloading",
                LoadedModelStatus::Error => "error",
            },
            "backend": handle.backend,
        }));
    }

    data.sort_by(|a, b| {
        a.get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .cmp(
                b.get("id")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default(),
            )
    });
    data
}

async fn chat_conversations(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let storage = state.storage.lock().await;
    match storage.list_chat_sessions() {
        Ok(sessions) => Json(sessions).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct CreateChatConversationRequest {
    title: String,
    model_id: Option<String>,
}

async fn create_chat_conversation(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<CreateChatConversationRequest>,
) -> impl IntoResponse {
    let title = body.title.trim();
    if title.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, "title is required").into_response();
    }

    let storage = state.storage.lock().await;
    match storage.create_chat_session(title, body.model_id) {
        Ok(session) => (axum::http::StatusCode::CREATED, Json(session)).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct RenameChatConversationRequest {
    id: Uuid,
    title: String,
}

async fn rename_chat_conversation(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<RenameChatConversationRequest>,
) -> impl IntoResponse {
    let title = body.title.trim();
    if title.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, "title is required").into_response();
    }

    let storage = state.storage.lock().await;
    match storage.rename_chat_session(body.id, title) {
        Ok(()) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct DeleteChatConversationRequest {
    id: Uuid,
}

async fn delete_chat_conversation(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<DeleteChatConversationRequest>,
) -> impl IntoResponse {
    let storage = state.storage.lock().await;
    match storage.delete_chat_session(body.id) {
        Ok(()) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct UpdateChatConversationModelRequest {
    id: Uuid,
    model_id: Option<String>,
}

async fn update_chat_conversation_model(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<UpdateChatConversationModelRequest>,
) -> impl IntoResponse {
    let storage = state.storage.lock().await;
    match storage.update_chat_session_model(body.id, body.model_id) {
        Ok(()) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct AppendChatMessageRequest {
    session_id: Uuid,
    role: ChatRole,
    content: String,
}

async fn append_chat_message(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<AppendChatMessageRequest>,
) -> impl IntoResponse {
    if body.content.trim().is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, "content is required").into_response();
    }

    let storage = state.storage.lock().await;
    match storage.append_chat_message(body.session_id, body.role, body.content) {
        Ok(message) => (axum::http::StatusCode::CREATED, Json(message)).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        )
            .into_response(),
    }
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
    #[serde(default, deserialize_with = "deserialize_stop_sequences")]
    pub stop: Vec<String>,
    pub seed: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenAiMessage {
    pub role: String,
    pub content: String,
}

fn deserialize_stop_sequences<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(Vec::new()),
        Some(serde_json::Value::String(stop)) => Ok(vec![stop]),
        Some(serde_json::Value::Array(items)) => items
            .into_iter()
            .map(|item| match item {
                serde_json::Value::String(stop) => Ok(stop),
                _ => Err(serde::de::Error::custom(
                    "stop must be a string or an array of strings",
                )),
            })
            .collect(),
        Some(_) => Err(serde::de::Error::custom(
            "stop must be a string or an array of strings",
        )),
    }
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
            stop: body.stop,
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
