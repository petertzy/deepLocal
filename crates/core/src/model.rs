use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelFormat {
    Gguf,
    Safetensors,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    Chat,
    Completion,
    Embeddings,
    Tools,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFile {
    pub filename: String,
    pub path: Option<String>,
    pub size_bytes: Option<u64>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDescriptor {
    pub id: String,
    pub name: String,
    pub family: Option<String>,
    pub source: String,
    pub repo: Option<String>,
    pub revision: Option<String>,
    pub format: ModelFormat,
    pub quantization: Option<String>,
    pub size_bytes: Option<u64>,
    pub context_length: Option<u32>,
    pub capabilities: Vec<ModelCapability>,
    pub files: Vec<ModelFile>,
    pub local_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ModelDescriptor {
    pub fn local_gguf(id: impl Into<String>, path: impl Into<String>) -> Self {
        let id = id.into();
        let path = path.into();
        let now = Utc::now();
        Self {
            name: id.clone(),
            id,
            family: None,
            source: "local".to_string(),
            repo: None,
            revision: None,
            format: ModelFormat::Gguf,
            quantization: None,
            size_bytes: None,
            context_length: None,
            capabilities: vec![ModelCapability::Chat, ModelCapability::Completion],
            files: vec![ModelFile {
                filename: path.clone(),
                path: Some(path.clone()),
                size_bytes: None,
                sha256: None,
            }],
            local_path: Some(path),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    Queued,
    Downloading,
    Paused,
    Downloaded,
    Error,
}
