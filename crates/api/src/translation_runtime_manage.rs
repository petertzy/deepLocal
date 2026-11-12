//! Personal source-checkout OPUS-MT runtime management.
//! Only the fixed repository-local .tools/opus-runtime directory is mutable.
use crate::{ApiState, translation_api::stop_worker};
use axum::{extract::State, http::{HeaderMap, StatusCode, header::ORIGIN}, response::IntoResponse, Json};
use serde::Deserialize;
use std::{path::{Path, PathBuf}, sync::{Arc, OnceLock}, time::Duration};
use tokio::{process::Command, sync::Mutex, time::timeout};

static RUNTIME_OPERATION: OnceLock<Mutex<()>> = OnceLock::new();
const MARKER: &str = ".deeplocal-opus-managed";

fn trusted_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(ORIGIN) else { return true; };
    matches!(origin.to_str().ok(), Some(
        "tauri://localhost" | "http://tauri.localhost" |
        "http://127.0.0.1:5173" | "http://localhost:5173"
    ))
}

fn runtime_dir() -> Result<PathBuf, String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    if !root.join("scripts/start-opus.sh").is_file() || !root.join("Cargo.toml").is_file() {
        return Err("Runtime management requires the deepLocal source checkout working directory.".into());
    }
    let tools = root.join(".tools");
    if tools.is_symlink() { return Err("Refusing a symlinked .tools directory.".into()); }
    std::fs::create_dir_all(&tools).map_err(|e| e.to_string())?;
    let tools = tools.canonicalize().map_err(|e| e.to_string())?;
    Ok(tools.join("opus-runtime"))
}

fn managed_target(dir: &Path) -> Result<(), String> {
    if dir.is_symlink() { return Err("Refusing a symlinked runtime directory.".into()); }
    if dir.exists() && !dir.join(MARKER).is_file() {
        return Err("Existing runtime is not marked as script-managed; refusing to modify it.".into());
    }
    Ok(())
}

fn python(dir: &Path) -> PathBuf {
    if cfg!(windows) { dir.join("Scripts/python.exe") } else { dir.join("bin/python") }
}

async fn run(program: &Path, args: &[&str], duration: Duration) -> Result<(), String> {
    let output = timeout(duration, Command::new(program).args(args).kill_on_drop(true).output())
        .await.map_err(|_| "Runtime operation timed out.".to_owned())?
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Runtime operation failed: {}", detail.chars().take(1200).collect::<String>()));
    }
    Ok(())
}

#[derive(Deserialize)]
pub(crate) struct RuntimeAction { action: String }

pub(crate) async fn manage(
    headers: HeaderMap,
    State(state): State<Arc<ApiState>>,
    Json(request): Json<RuntimeAction>,
) -> impl IntoResponse {
    if !trusted_origin(&headers) { return (StatusCode::FORBIDDEN, "Untrusted origin").into_response(); }
    if !matches!(request.action.as_str(), "install" | "repair" | "remove") {
        return (StatusCode::BAD_REQUEST, "Unsupported runtime action").into_response();
    }
    let lock = RUNTIME_OPERATION.get_or_init(|| Mutex::new(()));
    let Ok(_guard) = lock.try_lock() else {
        return (StatusCode::CONFLICT, "A runtime operation is already running").into_response();
    };
    let dir = match runtime_dir() {
        Ok(dir) => dir,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    if let Err(e) = managed_target(&dir) {
        return (StatusCode::CONFLICT, e).into_response();
    }
    let (selected, source) = crate::translation_runtime::selected_python(&state.models_directory);
    if source == "environment" && selected != python(&dir) {
        return (StatusCode::CONFLICT,
            "A custom Python interpreter is active. Restart without DEEPLOCAL_OPUS_MT_PYTHON to manage the local runtime.").into_response();
    }
    let result = match request.action.as_str() {
        "remove" => {
            if !dir.exists() { Err("No script-managed runtime is installed.".to_owned()) }
            else {
                let mut worker = state.translation_worker.lock().await;
                stop_worker(&mut worker).await;
                tokio::fs::remove_dir_all(&dir).await.map_err(|e| e.to_string())
            }
        }
        action => {
            if action == "install" && dir.exists() {
                Err("Runtime already installed. Use Repair to reinstall dependencies.".to_owned())
            } else {
                install(&dir, action == "repair").await
            }
        }
    };
    match result {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({
            "message": format!("Runtime {} completed. Refresh runtime status.", request.action)
        }))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message": e}))).into_response(),
    }
}

async fn install(dir: &Path, repair: bool) -> Result<(), String> {
    if !dir.exists() {
        let parent = dir.parent().ok_or("Invalid runtime directory")?;
        let staging = parent.join(format!(".opus-runtime-setup-{}", std::process::id()));
        if staging.exists() { return Err("An incomplete runtime setup directory exists.".into()); }
        let base = Path::new("python3");
        let result = async {
            let path = staging.to_str().ok_or("Invalid runtime path")?;
            run(base, &["-m", "venv", path], Duration::from_secs(90)).await?;
            run(&python(&staging), &["-m", "pip", "install", "ctranslate2==4.9.0", "sentencepiece==0.2.3"], Duration::from_secs(300)).await?;
            run(&python(&staging), &["-c", "import ctranslate2, sentencepiece"], Duration::from_secs(15)).await?;
            tokio::fs::write(staging.join(MARKER), b"deepLocal OPUS-MT managed runtime\n").await.map_err(|e| e.to_string())?;
            tokio::fs::rename(&staging, dir).await.map_err(|e| e.to_string())
        }.await;
        if result.is_err() { let _ = tokio::fs::remove_dir_all(&staging).await; }
        result
    } else {
        let interpreter = python(dir);
        if !interpreter.is_file() { return Err("Managed Python executable is missing.".into()); }
        let mut args = vec!["-m", "pip", "install"];
        if repair { args.push("--force-reinstall"); }
        args.extend(["ctranslate2==4.9.0", "sentencepiece==0.2.3"]);
        run(&interpreter, &args, Duration::from_secs(300)).await?;
        run(&interpreter, &["-c", "import ctranslate2, sentencepiece"], Duration::from_secs(15)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_path_uses_expected_interpreter() {
        let root = Path::new("/tmp/.tools/opus-runtime");
        assert!(python(root).ends_with(if cfg!(windows) { "Scripts/python.exe" } else { "bin/python" }));
    }
}
