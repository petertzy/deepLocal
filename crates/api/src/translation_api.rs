//! Opt-in, local-only OPUS-MT endpoint backed by CTranslate2.
use crate::{ApiState, translation_routing::{TranslationPair, TranslationRoute, select_route}};
use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::{io::{AsyncBufReadExt, AsyncWriteExt, BufReader}, process::{Child, ChildStdin, ChildStdout, Command}, sync::Mutex, time::timeout};

#[derive(Debug, Deserialize)]
pub(crate) struct TranslateRequest {
    text: String,
    source_language: String,
    target_language: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkerResponse {
    translation: Option<String>,
    error: Option<String>,
}

/// One long-lived Python process shared by both translation directions.
pub(crate) struct TranslationWorkerProcess {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

pub(crate) type TranslationWorkerState = Mutex<Option<TranslationWorkerProcess>>;

async fn request_translation(
    worker: &mut Option<TranslationWorkerProcess>,
    models_root: &std::path::Path,
    direction: &str,
    text: &str,
) -> Result<String, String> {
    if worker.is_none() {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/opus_mt_translate.py");
        let python = std::env::var("DEEPLOCAL_OPUS_MT_PYTHON")
            .unwrap_or_else(|_| "python3".to_owned());
        let mut child = Command::new(python)
            .arg(script)
            .arg("--models-root")
            .arg(models_root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format!("Cannot start OPUS-MT worker: {error}"))?;
        let stdin = child.stdin.take().ok_or("Missing worker stdin")?;
        let stdout = child.stdout.take().ok_or("Missing worker stdout")?;
        *worker = Some(TranslationWorkerProcess {
            _child: child,
            stdin,
            stdout: BufReader::new(stdout),
        });
    }
    let worker = worker.as_mut().ok_or("Worker unavailable")?;
    let input = serde_json::json!({"text": text, "direction": direction}).to_string();
    worker.stdin.write_all(input.as_bytes()).await.map_err(|e| e.to_string())?;
    worker.stdin.write_all(b"\\n").await.map_err(|e| e.to_string())?;
    worker.stdin.flush().await.map_err(|e| e.to_string())?;
    let mut line = String::new();
    if worker.stdout.read_line(&mut line).await.map_err(|e| e.to_string())? == 0 {
        return Err("OPUS-MT worker exited unexpectedly".into());
    }
    let response: WorkerResponse = serde_json::from_str(&line)
        .map_err(|e| format!("Invalid worker response: {e}"))?;
    match (response.translation, response.error) {
        (Some(translation), None) => Ok(translation),
        (_, Some(error)) => Err(format!("OPUS-MT worker failed: {}", error.chars().take(1024).collect::<String>())),
        _ => Err("Worker response contains no translation".into()),
    }
}

fn model_directory(root: &std::path::Path, pair: &TranslationPair) -> PathBuf {
    root.join("opus-mt").join(pair.model_key())
}

pub(crate) async fn translate(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<TranslateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pair = TranslationPair::new(&request.source_language, &request.target_language)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_owned()))?;
    if request.text.trim().is_empty() || request.text.len() > 16_384 {
        return Err((StatusCode::BAD_REQUEST, "text must contain 1 to 16384 bytes".into()));
    }
    let directory = model_directory(&state.models_directory, &pair);
    let installed = if ["model.bin", "source.spm", "target.spm"].iter()
        .all(|file| directory.join(file).is_file())
    {
        HashMap::from([(pair.model_key(), directory.to_string_lossy().into_owned())])
    } else {
        HashMap::new()
    };
    let TranslationRoute::LocalModel { .. } = select_route(&pair, &installed, false) else {
        return Err((StatusCode::NOT_FOUND,
            format!("No local OPUS-MT model installed for {}", pair.model_key())));
    };
    // Model files must remain inside the configured model root.
    let canonical_root = state.models_directory.canonicalize()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let canonical_model = directory.canonicalize()
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    if !canonical_model.starts_with(&canonical_root) {
        return Err((StatusCode::BAD_REQUEST, "model directory escapes models root".into()));
    }

    let mut guard = state.translation_worker.lock().await;
    let result = timeout(
        Duration::from_secs(120),
        request_translation(&mut guard, &canonical_root, pair.model_key(), &request.text),
    ).await;
    let translation = match result {
        Ok(Ok(translation)) => translation,
        Ok(Err(error)) => {
            // Discard broken workers so the next request starts a clean process.
            *guard = None;
            return Err((StatusCode::BAD_GATEWAY, error));
        }
        Err(_) => {
            *guard = None;
            return Err((StatusCode::GATEWAY_TIMEOUT, "OPUS-MT inference timed out".into()));
        }
    };
    drop(guard);
    Ok(Json(serde_json::json!({
        "translation": translation,
        "source_language": pair.source,
        "target_language": pair.target,
        "model": format!("opus-mt-{}", pair.model_key()),
        "provider": "local-ctranslate2"
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_paths_are_direction_specific() {
        let root = std::path::Path::new("/models");
        let forward = TranslationPair::new("en-US", "zh-CN").unwrap();
        let reverse = TranslationPair::new("zh", "en").unwrap();
        assert_eq!(model_directory(root, &forward), root.join("opus-mt/en-zh"));
        assert_eq!(model_directory(root, &reverse), root.join("opus-mt/zh-en"));
    }

    #[test]
    fn worker_response_requires_translation_field() {
        assert!(serde_json::from_str::<WorkerResponse>(r#"{"translation":"你好"}"#).is_ok());
        assert!(serde_json::from_str::<WorkerResponse>(r#"{"text":"你好"}"#).is_err());
    }
}
