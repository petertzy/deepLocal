use chrono::Utc;
use deeplocal_core::{
    ChatRole, DocumentChunk, DownloadJob, LocalDocument, ModelDescriptor, PromptPreset,
};
use deeplocal_storage::Storage;
use uuid::Uuid;

#[test]
fn stores_and_lists_models() {
    let storage = Storage::open_memory().expect("open storage");
    let model = ModelDescriptor::local_gguf("mock-model", "mock.gguf");

    storage.upsert_model(&model).expect("upsert model");
    let models = storage.list_models().expect("list models");

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "mock-model");
}

#[test]
fn manages_chat_sessions_and_messages() {
    let storage = Storage::open_memory().expect("open storage");

    let session = storage
        .create_chat_session("First chat", Some("mock-model".to_string()))
        .expect("create session");
    storage
        .append_chat_message(session.id, ChatRole::User, "hello")
        .expect("append user message");
    storage
        .append_chat_message(session.id, ChatRole::Assistant, "hi there")
        .expect("append assistant message");
    storage
        .rename_chat_session(session.id, "Renamed chat")
        .expect("rename session");

    let sessions = storage.list_chat_sessions().expect("list sessions");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].title, "Renamed chat");
    assert_eq!(sessions[0].model_id.as_deref(), Some("mock-model"));
    assert_eq!(sessions[0].messages.len(), 2);
    assert_eq!(sessions[0].messages[0].role, ChatRole::User);

    storage
        .delete_chat_session(session.id)
        .expect("delete session");
    assert!(
        storage
            .list_chat_sessions()
            .expect("list sessions")
            .is_empty()
    );
}

#[test]
fn manages_prompt_presets() {
    let storage = Storage::open_memory().expect("open storage");
    let now = Utc::now();
    let mut preset = PromptPreset {
        id: Uuid::new_v4(),
        name: "Code reviewer".to_string(),
        system_prompt: Some("Review code carefully and explain risks.".to_string()),
        prompt_template: Some("Review this code:\n{{code}}".to_string()),
        created_at: now,
        updated_at: now,
    };

    storage
        .upsert_prompt_preset(&preset)
        .expect("create preset");
    let presets = storage.list_prompt_presets().expect("list presets");
    assert_eq!(presets, vec![preset.clone()]);

    preset.name = "Senior code reviewer".to_string();
    preset.updated_at = Utc::now();
    storage
        .upsert_prompt_preset(&preset)
        .expect("update preset");
    assert_eq!(storage.list_prompt_presets().unwrap()[0].name, preset.name);
    assert!(storage.delete_prompt_preset(preset.id).unwrap());
    assert!(storage.list_prompt_presets().unwrap().is_empty());
    assert!(!storage.delete_prompt_preset(preset.id).unwrap());
}

#[test]
fn stores_lists_and_clears_download_jobs() {
    let storage = Storage::open_memory().expect("open storage");
    let now = Utc::now();
    let job = DownloadJob {
        id: "job-1".to_string(),
        repo: "google/gemma".to_string(),
        filename: "model.gguf".to_string(),
        status: "error".to_string(),
        downloaded_bytes: 42,
        total_bytes: Some(100),
        speed_bytes_per_sec: Some(12.5),
        eta_seconds: Some(5),
        local_path: Some("./models/model.gguf".to_string()),
        error: Some("network failed".to_string()),
        cancel_requested: false,
        created_at: now,
        updated_at: now,
    };

    storage
        .upsert_download_job(&job)
        .expect("upsert download job");
    let jobs = storage
        .list_recent_download_jobs(10)
        .expect("list download jobs");

    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, "job-1");
    assert_eq!(jobs[0].status, "error");
    assert_eq!(jobs[0].downloaded_bytes, 42);
    assert_eq!(jobs[0].total_bytes, Some(100));
    assert_eq!(jobs[0].local_path.as_deref(), Some("./models/model.gguf"));
    assert_eq!(jobs[0].error.as_deref(), Some("network failed"));

    let cleared = storage
        .delete_download_jobs_by_statuses(&["downloaded", "cancelled", "error"])
        .expect("clear history");
    assert_eq!(cleared, 1);
    assert!(
        storage
            .list_recent_download_jobs(10)
            .expect("list jobs")
            .is_empty()
    );
}

#[test]
fn replaces_documents_and_persists_local_embeddings() {
    let storage = Storage::open_memory().expect("open storage");
    let now = Utc::now();
    let first_id = Uuid::new_v4();
    let document = LocalDocument {
        id: first_id,
        name: "notes.md".to_string(),
        source_key: "upload:notes.md".to_string(),
        character_count: 42,
        chunk_count: 1,
        created_at: now,
        updated_at: now,
    };
    let first_chunk = DocumentChunk {
        id: Uuid::new_v4(),
        document_id: first_id,
        chunk_index: 0,
        content: "Local documents stay on this device.".to_string(),
        embedding: vec![0.0, 0.75, -0.25],
    };

    storage
        .replace_document(&document, &[first_chunk])
        .expect("store document");
    let documents = storage.list_documents().expect("list documents");
    let chunks = storage
        .list_indexed_document_chunks()
        .expect("list document chunks");
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].name, "notes.md");
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].embedding, vec![0.0, 0.75, -0.25]);

    let replacement_id = Uuid::new_v4();
    let replacement = LocalDocument {
        id: replacement_id,
        name: "notes.md".to_string(),
        source_key: "upload:notes.md".to_string(),
        character_count: 21,
        chunk_count: 1,
        created_at: now,
        updated_at: now,
    };
    storage
        .replace_document(
            &replacement,
            &[DocumentChunk {
                id: Uuid::new_v4(),
                document_id: replacement_id,
                chunk_index: 0,
                content: "Replacement content.".to_string(),
                embedding: vec![1.0, 0.0, 0.0],
            }],
        )
        .expect("replace document");

    let documents = storage.list_documents().expect("list replacement");
    let chunks = storage
        .list_indexed_document_chunks()
        .expect("list replacement chunks");
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].id, replacement_id);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].content, "Replacement content.");

    assert!(
        storage
            .delete_document(replacement_id)
            .expect("delete document")
    );
    assert!(
        storage
            .list_documents()
            .expect("list removed documents")
            .is_empty()
    );
}
