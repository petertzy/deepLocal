use crate::{GeneratedToken, GenerationRequest, ModelDescriptor, ModelFormat};
use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadOptions {
    pub context_length: Option<u32>,
    pub gpu_layers: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHandle {
    pub id: String,
    pub backend: String,
    pub status: LoadedModelStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStatus {
    pub id: String,
    pub available: bool,
    pub binary_path: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadedModelStatus {
    Loading,
    Loaded,
    Unloading,
    Error,
}

#[async_trait]
pub trait InferenceBackend: Send + Sync {
    fn id(&self) -> &str;
    fn supported_formats(&self) -> Vec<ModelFormat>;
    fn status(&self) -> BackendStatus;
    async fn load(
        &self,
        model: ModelDescriptor,
        options: LoadOptions,
    ) -> anyhow::Result<ModelHandle>;
    async fn generate(
        &self,
        request: GenerationRequest,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<GeneratedToken>>>;
    async fn unload(&self, model_id: &str) -> anyhow::Result<()>;
}
