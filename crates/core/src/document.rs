use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalDocument {
    pub id: Uuid,
    pub name: String,
    #[serde(skip_serializing)]
    pub source_key: String,
    pub character_count: u64,
    pub chunk_count: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct DocumentChunk {
    pub id: Uuid,
    pub document_id: Uuid,
    pub chunk_index: u32,
    pub content: String,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct IndexedDocumentChunk {
    pub id: Uuid,
    pub document_id: Uuid,
    pub document_name: String,
    pub chunk_index: u32,
    pub content: String,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrievedDocumentChunk {
    pub document_id: Uuid,
    pub document_name: String,
    pub chunk_index: u32,
    pub content: String,
    pub score: f32,
}
