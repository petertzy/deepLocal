use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptPreset {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub built_in_key: Option<String>,
    pub system_prompt: Option<String>,
    pub prompt_template: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PromptPreset {
    pub const DEFAULT_ASSISTANT_ID: Uuid = Uuid::from_u128(0xdee10ca1000040008000000000000001);
    pub const DEFAULT_ASSISTANT_KEY: &'static str = "default_assistant";
    pub const DEFAULT_ASSISTANT_NAME: &'static str = "Default assistant";
    pub const DEFAULT_ASSISTANT_SYSTEM_PROMPT: &'static str = "You are a helpful assistant. Give accurate, clear, and useful answers. Be transparent about uncertainty and do not invent facts.";

    pub fn default_assistant() -> Self {
        let now = Utc::now();
        Self {
            id: Self::DEFAULT_ASSISTANT_ID,
            name: Self::DEFAULT_ASSISTANT_NAME.to_string(),
            built_in_key: Some(Self::DEFAULT_ASSISTANT_KEY.to_string()),
            system_prompt: Some(Self::DEFAULT_ASSISTANT_SYSTEM_PROMPT.to_string()),
            prompt_template: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn restore_defaults(&mut self) -> bool {
        if self.built_in_key.as_deref() != Some(Self::DEFAULT_ASSISTANT_KEY) {
            return false;
        }
        self.name = Self::DEFAULT_ASSISTANT_NAME.to_string();
        self.system_prompt = Some(Self::DEFAULT_ASSISTANT_SYSTEM_PROMPT.to_string());
        self.prompt_template = None;
        self.updated_at = Utc::now();
        true
    }
}
