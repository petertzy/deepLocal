use async_trait::async_trait;
use deeplocal_core::{
    BackendStatus, ChatRole, GeneratedToken, GenerationRequest, InferenceBackend, LoadOptions,
    LoadedModelStatus, ModelDescriptor, ModelFormat, ModelHandle,
};
use futures::{StreamExt, stream::BoxStream};
use std::{
    collections::HashMap,
    path::PathBuf,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicU16, Ordering},
    },
    time::Duration,
};
use tokio::sync::RwLock;
use tokio::{process::Command, time::sleep};
use tokio_stream as stream;

#[derive(Clone, Default)]
pub struct RuntimeManager {
    backends: Arc<RwLock<HashMap<String, Arc<dyn InferenceBackend>>>>,
    models: Arc<RwLock<HashMap<String, ModelDescriptor>>>,
    loaded: Arc<RwLock<HashMap<String, ModelHandle>>>,
}

impl RuntimeManager {
    pub async fn register_backend(&self, backend: Arc<dyn InferenceBackend>) {
        self.backends
            .write()
            .await
            .insert(backend.id().to_string(), backend);
    }

    pub async fn register_model(&self, model: ModelDescriptor) {
        self.models.write().await.insert(model.id.clone(), model);
    }

    pub async fn list_models(&self) -> Vec<ModelDescriptor> {
        let mut models: Vec<_> = self.models.read().await.values().cloned().collect();
        models.sort_by(|a, b| a.id.cmp(&b.id));
        models
    }

    pub async fn get_model(&self, model_id: &str) -> Option<ModelDescriptor> {
        self.models.read().await.get(model_id).cloned()
    }

    pub async fn remove_model(&self, model_id: &str) -> Option<ModelDescriptor> {
        self.models.write().await.remove(model_id)
    }

    pub async fn is_model_loaded(&self, model_id: &str) -> bool {
        self.loaded.read().await.contains_key(model_id)
    }

    pub async fn load_model(
        &self,
        backend_id: &str,
        model: ModelDescriptor,
        options: LoadOptions,
    ) -> anyhow::Result<ModelHandle> {
        let backend = self
            .backends
            .read()
            .await
            .get(backend_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("backend not registered: {backend_id}"))?;
        self.register_model(model.clone()).await;
        let handle = backend.load(model, options).await?;
        self.loaded
            .write()
            .await
            .insert(handle.id.clone(), handle.clone());
        Ok(handle)
    }

    pub async fn unload_model(&self, model_id: &str) -> anyhow::Result<()> {
        let handle = self
            .loaded
            .write()
            .await
            .remove(model_id)
            .ok_or_else(|| anyhow::anyhow!("model is not loaded: {model_id}"))?;
        let backend = self
            .backends
            .read()
            .await
            .get(&handle.backend)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("backend not registered: {}", handle.backend))?;
        backend.unload(model_id).await
    }

    pub async fn list_loaded_models(&self) -> Vec<ModelHandle> {
        self.loaded.read().await.values().cloned().collect()
    }

    pub async fn list_backend_statuses(&self) -> Vec<BackendStatus> {
        let mut statuses: Vec<_> = self
            .backends
            .read()
            .await
            .values()
            .map(|backend| backend.status())
            .collect();
        statuses.sort_by(|a, b| a.id.cmp(&b.id));
        statuses
    }

    pub async fn generate(
        &self,
        request: GenerationRequest,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<GeneratedToken>>> {
        let handle = self
            .loaded
            .read()
            .await
            .get(&request.model)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("model is not loaded: {}", request.model))?;
        let backend = self
            .backends
            .read()
            .await
            .get(&handle.backend)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("backend not registered: {}", handle.backend))?;
        backend.generate(request).await
    }

    pub async fn load_registered_model(
        &self,
        backend_id: &str,
        model_id: &str,
        options: LoadOptions,
    ) -> anyhow::Result<ModelHandle> {
        let model = self
            .get_model(model_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("model is not registered: {model_id}"))?;
        self.load_model(backend_id, model, options).await
    }
}

#[derive(Default)]
pub struct MockBackend;

#[async_trait]
impl InferenceBackend for MockBackend {
    fn id(&self) -> &str {
        "mock"
    }

    fn supported_formats(&self) -> Vec<ModelFormat> {
        vec![ModelFormat::Gguf]
    }

    fn status(&self) -> BackendStatus {
        BackendStatus {
            id: self.id().to_string(),
            available: true,
            binary_path: None,
            error: None,
        }
    }

    async fn load(
        &self,
        model: ModelDescriptor,
        _options: LoadOptions,
    ) -> anyhow::Result<ModelHandle> {
        Ok(ModelHandle {
            id: model.id,
            backend: self.id().to_string(),
            status: LoadedModelStatus::Loaded,
        })
    }

    async fn generate(
        &self,
        request: GenerationRequest,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<GeneratedToken>>> {
        let prompt = request
            .messages
            .last()
            .map(|message| message.content.clone())
            .unwrap_or_default();
        let text = format!("deepLocal mock response: {prompt}");
        let mut tokens: Vec<_> = text
            .split_whitespace()
            .enumerate()
            .map(|(index, word)| {
                Ok(GeneratedToken {
                    text: format!("{word} "),
                    index,
                    done: false,
                })
            })
            .collect();
        tokens.push(Ok(GeneratedToken {
            text: String::new(),
            index: tokens.len(),
            done: true,
        }));
        Ok(stream::iter(tokens).boxed())
    }

    async fn unload(&self, _model_id: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct LlamaCppBackend {
    binary: PathBuf,
    next_port: AtomicU16,
    ready_attempts: usize,
    ready_interval: Duration,
    processes: RwLock<HashMap<String, LlamaProcess>>,
}

struct LlamaProcess {
    port: u16,
    child: tokio::process::Child,
}

fn resolve_binary_path(binary: &PathBuf) -> Option<PathBuf> {
    if binary.components().count() > 1 || binary.is_absolute() {
        return binary.exists().then(|| binary.clone());
    }

    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|path| path.join(binary))
            .find(|candidate| candidate.exists())
    })
}

fn llama_startup_error_message(wait_error: &str, stderr: Option<&[u8]>) -> String {
    let stderr = stderr
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .unwrap_or("")
        .trim();
    let details = stderr.to_lowercase();

    if details.contains("context") || details.contains("ctx") || details.contains("n_ctx") {
        return "llama-server could not start. Try a smaller context size.".to_string();
    }
    if details.contains("no such file")
        || details.contains("cannot open")
        || details.contains("failed to open")
        || details.contains("model")
    {
        return "llama-server could not open the model file. Check the model path and file."
            .to_string();
    }
    if details.contains("memory") || details.contains("alloc") || details.contains("out of memory")
    {
        return "llama-server ran out of memory. Try fewer GPU layers or a smaller model."
            .to_string();
    }
    if !stderr.is_empty() {
        let first_line = stderr.lines().next().unwrap_or(stderr).trim();
        return format!(
            "llama-server could not start: {}",
            truncate_error(first_line, 140)
        );
    }

    format!(
        "llama-server could not start: {}",
        truncate_error(wait_error, 140)
    )
}

fn truncate_error(message: &str, max_len: usize) -> String {
    if message.len() <= max_len {
        return message.to_string();
    }
    format!("{}...", &message[..max_len.saturating_sub(3)])
}

#[cfg(test)]
mod tests {
    use super::llama_startup_error_message;

    #[test]
    fn startup_errors_suggest_smaller_context_for_context_failures() {
        let message = llama_startup_error_message("not ready", Some(b"invalid n_ctx value"));
        assert!(message.contains("smaller context"));
    }

    #[test]
    fn startup_errors_suggest_model_path_for_open_failures() {
        let message = llama_startup_error_message("not ready", Some(b"failed to open model"));
        assert!(message.contains("model path"));
    }

    #[test]
    fn startup_errors_suggest_memory_adjustments_for_allocation_failures() {
        let message = llama_startup_error_message("not ready", Some(b"out of memory"));
        assert!(message.contains("fewer GPU layers"));
    }
}

impl LlamaCppBackend {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            next_port: AtomicU16::new(18080),
            ready_attempts: 120,
            ready_interval: Duration::from_millis(500),
            processes: RwLock::new(HashMap::new()),
        }
    }

    pub fn new_for_tests(binary: impl Into<PathBuf>, ready_attempts: usize) -> Self {
        Self {
            binary: binary.into(),
            next_port: AtomicU16::new(18080),
            ready_attempts,
            ready_interval: Duration::from_millis(1),
            processes: RwLock::new(HashMap::new()),
        }
    }

    pub async fn process_count(&self) -> usize {
        self.processes.read().await.len()
    }

    pub fn from_env() -> Self {
        let binary = std::env::var("DEELOCAL_LLAMA_SERVER")
            .or_else(|_| std::env::var("DEEPLOCAL_LLAMA_SERVER"))
            .or_else(|_| std::env::var("LLAMA_SERVER"))
            .unwrap_or_else(|_| "llama-server".to_string());
        Self::new(binary)
    }

    async fn wait_until_ready(&self, port: u16) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/health");
        for _ in 0..self.ready_attempts {
            if let Ok(response) = client.get(&url).send().await {
                if response.status().is_success() {
                    return Ok(());
                }
            }
            sleep(self.ready_interval).await;
        }
        anyhow::bail!("llama-server did not become ready on port {port}");
    }
}

impl Drop for LlamaCppBackend {
    fn drop(&mut self) {
        for (_, mut process) in self.processes.get_mut().drain() {
            let _ = process.child.start_kill();
        }
    }
}

#[async_trait]
impl InferenceBackend for LlamaCppBackend {
    fn id(&self) -> &str {
        "llama.cpp"
    }

    fn supported_formats(&self) -> Vec<ModelFormat> {
        vec![ModelFormat::Gguf]
    }

    fn status(&self) -> BackendStatus {
        match resolve_binary_path(&self.binary) {
            Some(path) => BackendStatus {
                id: self.id().to_string(),
                available: true,
                binary_path: Some(path.to_string_lossy().to_string()),
                error: None,
            },
            None => BackendStatus {
                id: self.id().to_string(),
                available: false,
                binary_path: Some(self.binary.to_string_lossy().to_string()),
                error: Some(
                    "llama-server was not found. Install llama.cpp or set LLAMA_SERVER / DEEPLOCAL_LLAMA_SERVER to the llama-server binary."
                        .to_string(),
                ),
            },
        }
    }

    async fn load(
        &self,
        model: ModelDescriptor,
        options: LoadOptions,
    ) -> anyhow::Result<ModelHandle> {
        let model_path = model
            .local_path
            .clone()
            .or_else(|| model.files.iter().find_map(|file| file.path.clone()))
            .ok_or_else(|| {
                anyhow::anyhow!("The selected model does not have a local file path.")
            })?;
        if !PathBuf::from(&model_path).exists() {
            anyhow::bail!("The model file could not be found. Check the file path and try again.");
        }

        self.unload(&model.id).await.ok();
        let port = self.next_port.fetch_add(1, Ordering::Relaxed);
        let mut command = Command::new(&self.binary);
        command
            .arg("--model")
            .arg(&model_path)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(context) = options.context_length {
            command.arg("--ctx-size").arg(context.to_string());
        }
        if let Some(gpu_layers) = options.gpu_layers {
            command.arg("--n-gpu-layers").arg(gpu_layers.to_string());
        }

        let mut child = command.spawn().map_err(|error| {
            anyhow::anyhow!(
                "llama-server could not start at '{}': {error}. Install llama.cpp or set LLAMA_SERVER / DEEPLOCAL_LLAMA_SERVER.",
                self.binary.display()
            )
        })?;
        if let Err(error) = self.wait_until_ready(port).await {
            let _ = child.kill().await;
            let stderr = child
                .wait_with_output()
                .await
                .ok()
                .map(|output| output.stderr);
            return Err(anyhow::anyhow!(llama_startup_error_message(
                &error.to_string(),
                stderr.as_deref()
            )));
        }
        self.processes
            .write()
            .await
            .insert(model.id.clone(), LlamaProcess { port, child });

        Ok(ModelHandle {
            id: model.id,
            backend: self.id().to_string(),
            status: LoadedModelStatus::Loaded,
        })
    }

    async fn generate(
        &self,
        request: GenerationRequest,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<GeneratedToken>>> {
        let port = self
            .processes
            .read()
            .await
            .get(&request.model)
            .map(|process| process.port)
            .ok_or_else(|| anyhow::anyhow!("llama.cpp model is not loaded: {}", request.model))?;
        let messages: Vec<_> = request
            .messages
            .iter()
            .map(|message| {
                serde_json::json!({
                    "role": match message.role {
                        ChatRole::System => "system",
                        ChatRole::User => "user",
                        ChatRole::Assistant => "assistant",
                        ChatRole::Tool => "tool",
                    },
                    "content": message.content
                })
            })
            .collect();
        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .json(&serde_json::json!({
                "model": request.model,
                "stream": false,
                "messages": messages,
                "temperature": request.parameters.temperature,
                "top_p": request.parameters.top_p,
                "max_tokens": request.parameters.max_tokens
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;
        let text = response["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let tokens = vec![
            Ok(GeneratedToken {
                text,
                index: 0,
                done: false,
            }),
            Ok(GeneratedToken {
                text: String::new(),
                index: 1,
                done: true,
            }),
        ];
        Ok(stream::iter(tokens).boxed())
    }

    async fn unload(&self, model_id: &str) -> anyhow::Result<()> {
        if let Some(mut process) = self.processes.write().await.remove(model_id) {
            process.child.kill().await.ok();
        }
        Ok(())
    }
}
