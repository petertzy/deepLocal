use deeplocal_core::{DeepLocalConfig, ModelDescriptor};
use deeplocal_runtime::{LlamaCppBackend, MockBackend, RuntimeManager};
use std::{fs, path::PathBuf, sync::Arc};
use tauri::{Manager, State};

const API_HOST: &str = "127.0.0.1";

struct ApiAddress(String);

fn main() {
    tracing_subscriber::fmt::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let models_directory = desktop_models_directory(app)?;
            fs::create_dir_all(&models_directory)?;

            let resource_dir = app
                .path()
                .resource_dir()
                .map_err(|error| anyhow::anyhow!(error))?;
            configure_llama_server(&resource_dir);

            let listener =
                tauri::async_runtime::block_on(tokio::net::TcpListener::bind((API_HOST, 0)))?;
            let port = listener.local_addr()?.port();
            app.manage(ApiAddress(format!("http://{API_HOST}:{port}")));
            tauri::async_runtime::spawn(start_api(models_directory, listener, port));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api_base_url])
        .run(tauri::generate_context!())
        .expect("failed to run deepLocal desktop app");
}

#[tauri::command]
fn api_base_url(address: State<'_, ApiAddress>) -> String {
    address.0.clone()
}

fn desktop_models_directory(_app: &tauri::App) -> anyhow::Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
            anyhow::anyhow!("could not determine the current user's home directory")
        })?;
        Ok(home
            .join("Library")
            .join("Application Support")
            .join("deepLocal")
            .join("models"))
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(_app
            .path()
            .app_data_dir()
            .map_err(|error| anyhow::anyhow!(error))?
            .join("models"))
    }
}

async fn start_api(models_directory: PathBuf, listener: tokio::net::TcpListener, port: u16) {
    if let Err(error) = start_api_inner(models_directory, listener, port).await {
        eprintln!("deepLocal API failed: {error:#}");
    }
}

async fn start_api_inner(
    models_directory: PathBuf,
    listener: tokio::net::TcpListener,
    port: u16,
) -> anyhow::Result<()> {
    let mut config = DeepLocalConfig::default();
    config.models.directory = models_directory.clone();
    config.server.host = API_HOST.to_string();
    config.server.port = port;
    config.server.enable_cors = true;

    let runtime = RuntimeManager::default();
    runtime.register_backend(Arc::new(MockBackend)).await;
    runtime
        .register_backend(Arc::new(LlamaCppBackend::from_env()))
        .await;
    register_local_gguf_models(&runtime, &config.models.directory).await?;

    let app = deeplocal_api::router_with_models_directory(
        runtime,
        true,
        config.search_filters,
        config.models.directory.clone(),
    );
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
    let mut candidates = vec![resources.join("llama-runtime").join("llama-server")];
    if let Some(path) = std::env::var_os("DEEPLOCAL_LLAMA_SERVER") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(path) = std::env::var_os("LLAMA_SERVER") {
        candidates.push(PathBuf::from(path));
    }
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/llama-server"),
        PathBuf::from("/usr/local/bin/llama-server"),
    ]);

    for candidate in candidates {
        if candidate.exists() {
            // SAFETY: This runs during single-threaded app setup before any model
            // loading occurs, so no concurrent environment reads depend on it yet.
            unsafe {
                std::env::set_var("LLAMA_SERVER", &candidate);
            }
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::API_HOST;

    #[tokio::test]
    async fn packaged_api_uses_an_os_assigned_loopback_port() {
        let listener = tokio::net::TcpListener::bind((API_HOST, 0))
            .await
            .expect("bind an OS-assigned port");
        let address = listener.local_addr().expect("read assigned address");

        assert_eq!(address.ip().to_string(), API_HOST);
        assert_ne!(address.port(), 0);
        assert_ne!(address.port(), 14567);
    }
}
