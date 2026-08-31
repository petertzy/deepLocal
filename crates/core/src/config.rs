use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepLocalConfig {
    pub server: ServerConfig,
    pub models: ModelsConfig,
    pub runtime: RuntimeConfig,
    pub downloads: DownloadsConfig,
}

impl Default for DeepLocalConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            models: ModelsConfig::default(),
            runtime: RuntimeConfig::default(),
            downloads: DownloadsConfig::default(),
        }
    }
}

impl DeepLocalConfig {
    pub fn load(path: Option<PathBuf>) -> anyhow::Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let contents = fs::read_to_string(path)?;
        Ok(toml::from_str(&contents)?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub enable_cors: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 14567,
            enable_cors: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsConfig {
    pub directory: PathBuf,
    pub auto_load_last_model: bool,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("./models"),
            auto_load_last_model: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub default_backend: String,
    pub idle_unload_minutes: u32,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            default_backend: "mock".to_string(),
            idle_unload_minutes: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadsConfig {
    pub max_parallel: usize,
}

impl Default for DownloadsConfig {
    fn default() -> Self {
        Self { max_parallel: 2 }
    }
}
