use deeplocal_core::{DeepLocalConfig, ModelDescriptor};
use deeplocal_runtime::{LlamaCppBackend, MockBackend, RuntimeManager};
use std::{fs, net::SocketAddr, path::PathBuf, sync::Arc};
use tauri::Manager;

const API_HOST: &str = "127.0.0.1";
const API_PORT: u16 = 14567;

fn main() {
    tracing_subscriber::fmt::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| anyhow::anyhow!(error))?;
            fs::create_dir_all(data_dir.join("models"))?;
            std::env::set_current_dir(&data_dir)?;
            let resource_dir = app
                .path()
                .resource_dir()
                .map_err(|error| anyhow::anyhow!(error))?;
            configure_llama_server(&resource_dir);
            tauri::async_runtime::spawn(start_api(data_dir));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run deepLocal desktop app");
}

async fn start_api(data_dir: PathBuf) {
    if let Err(error) = start_api_inner(data_dir).await {
        eprintln!("deepLocal API failed: {error:#}");
    }
}

async fn start_api_inner(data_dir: PathBuf) -> anyhow::Result<()> {
    let mut config = DeepLocalConfig::default();
    config.models.directory = data_dir.join("models");
    config.server.host = API_HOST.to_string();
    config.server.port = API_PORT;
    config.server.enable_cors = true;

    let runtime = RuntimeManager::default();
    runtime.register_backend(Arc::new(MockBackend)).await;
    runtime
        .register_backend(Arc::new(LlamaCppBackend::from_env()))
        .await;
    register_local_gguf_models(&runtime, &config.models.directory).await?;

    let app = deeplocal_api::router_with_options(runtime, true, config.search_filters);
    let addr: SocketAddr = format!("{API_HOST}:{API_PORT}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn register_local_gguf_models(
    runtime: &RuntimeManager,
    directory: &PathBuf,
) -> anyhow::Result<()> {
    for path in collect_gguf_files(directory)? {
        let id = path
            .strip_prefix(directory)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, ":");
        runtime
            .register_model(ModelDescriptor::local_gguf(
                id,
                path.to_string_lossy().to_string(),
            ))
            .await;
    }
    Ok(())
}

fn collect_gguf_files(directory: &PathBuf) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if directory.exists() {
        collect_gguf_files_inner(directory, &mut files)?;
    }
    Ok(files)
}

fn collect_gguf_files_inner(directory: &PathBuf, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_gguf_files_inner(&path, files)?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn configure_llama_server(resources: &PathBuf) {
    if std::env::var_os("LLAMA_SERVER").is_some()
        || std::env::var_os("DEEPLOCAL_LLAMA_SERVER").is_some()
    {
        return;
    }

    for candidate in [
        resources.join("llama-server"),
        PathBuf::from("/opt/homebrew/bin/llama-server"),
        PathBuf::from("/usr/local/bin/llama-server"),
    ] {
        if candidate.exists() {
            // SAFETY: This runs during single-threaded app setup before any model
            // loading occurs, so no concurrent environment reads depend on it yet.
            unsafe {
                std::env::set_var("LLAMA_SERVER", candidate);
            }
            break;
        }
    }
}
