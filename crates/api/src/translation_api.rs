//! Opt-in, local-only OPUS-MT endpoint backed by CTranslate2.
use crate::{ApiState, translation_routing::{TranslationPair, TranslationRoute, select_route}};
use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command, time::timeout};

#[derive(Debug, Deserialize)]
pub(crate) struct TranslateRequest {
    text: String,
    source_language: String,
    target_language: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkerResponse {
    translation: String,
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

    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/opus_mt_translate.py");
    let python = std::env::var("DEEPLOCAL_OPUS_MT_PYTHON")
        .unwrap_or_else(|_| "python3".to_owned());
    let mut child = Command::new(python)
        .arg(script)
        .arg("--model-dir")
        .arg(canonical_model)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| (StatusCode::SERVICE_UNAVAILABLE,
            format!("Cannot start OPUS-MT worker: {error}")))?;
    let input = serde_json::json!({ "text": request.text }).to_string();
    let mut stdin = child.stdin.take().expect("worker stdin configured");
    stdin.write_all(input.as_bytes()).await
        .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
    drop(stdin);
    let output = timeout(Duration::from_secs(120), child.wait_with_output()).await
        .map_err(|_| (StatusCode::GATEWAY_TIMEOUT, "OPUS-MT inference timed out".into()))?
        .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err((StatusCode::BAD_GATEWAY,
            format!("OPUS-MT worker failed: {}", error.chars().take(1024).collect::<String>())));
    }
    let response: WorkerResponse = serde_json::from_slice(&output.stdout)
        .map_err(|error| (StatusCode::BAD_GATEWAY, format!("Invalid worker response: {error}")))?;
    Ok(Json(serde_json::json!({
        "translation": response.translation,
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
