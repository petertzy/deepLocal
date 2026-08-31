use async_trait::async_trait;
use deeplocal_core::{
    ChatRole, GeneratedToken, GenerationRequest, InferenceBackend, LoadOptions, LoadedModelStatus,
    ModelDescriptor, ModelFormat, ModelHandle,
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
    processes: RwLock<HashMap<String, LlamaProcess>>,
}

struct LlamaProcess {
    port: u16,
    child: tokio::process::Child,
}

impl LlamaCppBackend {
    pub fn from_env() -> Self {
        let binary = std::env::var("DEELOCAL_LLAMA_SERVER")
            .or_else(|_| std::env::var("DEEPLOCAL_LLAMA_SERVER"))
            .or_else(|_| std::env::var("LLAMA_SERVER"))
            .unwrap_or_else(|_| "llama-server".to_string());
        Self {
            binary: PathBuf::from(binary),
            next_port: AtomicU16::new(18080),
            processes: RwLock::new(HashMap::new()),
        }
    }

    async fn wait_until_ready(&self, port: u16) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/health");
        for _ in 0..120 {
            if let Ok(response) = client.get(&url).send().await {
                if response.status().is_success() {
                    return Ok(());
                }
            }
            sleep(Duration::from_millis(500)).await;
        }
        anyhow::bail!("llama-server did not become ready on port {port}");
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

    async fn load(
        &self,
        model: ModelDescriptor,
        options: LoadOptions,
    ) -> anyhow::Result<ModelHandle> {
        let model_path = model
            .local_path
            .clone()
            .or_else(|| model.files.iter().find_map(|file| file.path.clone()))
            .ok_or_else(|| anyhow::anyhow!("model has no local path: {}", model.id))?;
        if !PathBuf::from(&model_path).exists() {
            anyhow::bail!("model file does not exist: {model_path}");
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

        let child = command.spawn().map_err(|error| {
            anyhow::anyhow!(
                "failed to start llama-server at '{}': {error}. Install llama.cpp and set LLAMA_SERVER or DEEPLOCAL_LLAMA_SERVER.",
                self.binary.display()
            )
        })?;
        self.wait_until_ready(port).await?;
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
