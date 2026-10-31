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
        built_in_key: None,
        system_prompt: Some("Review code carefully and explain risks.".to_string()),
        prompt_template: Some("Review this code:\n{{code}}".to_string()),
        created_at: now,
        updated_at: now,
    };

    storage
        .upsert_prompt_preset(&preset)
        .expect("create preset");
    let presets = storage.list_prompt_presets().expect("list presets");
    assert!(presets.contains(&preset));
    assert_eq!(presets.len(), 2);

    preset.name = "Senior code reviewer".to_string();
    preset.updated_at = Utc::now();
    storage
        .upsert_prompt_preset(&preset)
        .expect("update preset");
    assert!(
        storage
            .list_prompt_presets()
            .unwrap()
            .iter()
            .any(|item| item.id == preset.id && item.name == preset.name)
    );
    assert!(storage.delete_prompt_preset(preset.id).unwrap());
    assert_eq!(storage.list_prompt_presets().unwrap().len(), 1);
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
        .replace_document(&document, &[first_chunk], None)
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
            None,
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

#[test]
fn fresh_storage_seeds_a_stable_builtin_preset() {
    let storage = Storage::open_memory().expect("open storage");
    let presets = storage.list_prompt_presets().expect("list presets");
    assert_eq!(presets.len(), 1);
    assert_eq!(presets[0].id, PromptPreset::DEFAULT_ASSISTANT_ID);
    assert_eq!(
        presets[0].built_in_key.as_deref(),
        Some(PromptPreset::DEFAULT_ASSISTANT_KEY)
    );
    assert_eq!(presets[0].name, PromptPreset::DEFAULT_ASSISTANT_NAME);
    assert_eq!(
        presets[0].system_prompt.as_deref(),
        Some(PromptPreset::DEFAULT_ASSISTANT_SYSTEM_PROMPT)
    );
}

#[test]
fn reopening_storage_preserves_custom_presets_and_builtin_changes() {
    let path =
        std::env::temp_dir().join(format!("deeplocal-preset-migration-{}.db", Uuid::new_v4()));
    let now = Utc::now();
    let custom = PromptPreset {
        id: Uuid::new_v4(),
        name: "Default assistant".to_string(),
        built_in_key: None,
        system_prompt: Some("Do not replace these instructions.".to_string()),
        prompt_template: Some("Keep this template.".to_string()),
        created_at: now,
        updated_at: now,
    };
    {
        let conn = rusqlite::Connection::open(&path).expect("open legacy database");
        conn.execute_batch("create table prompt_presets (id text primary key, name text not null, system_prompt text, prompt_template text, created_at text not null, updated_at text not null);").expect("create legacy schema");
        conn.execute(
            "insert into prompt_presets (id, name, system_prompt, prompt_template, created_at, updated_at) values (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![custom.id.to_string(), custom.name, custom.system_prompt, custom.prompt_template, now.to_rfc3339(), now.to_rfc3339()],
        ).expect("insert legacy preset");
    }
    {
        let storage = Storage::open(&path).expect("migrate legacy database");
        let presets = storage
            .list_prompt_presets()
            .expect("list migrated presets");
        assert_eq!(presets.len(), 2);
        assert!(presets.contains(&custom));
        let mut built_in = presets
            .into_iter()
            .find(|preset| preset.id == PromptPreset::DEFAULT_ASSISTANT_ID)
            .expect("built-in preset");
        built_in.name = "My renamed assistant".to_string();
        built_in.system_prompt = Some("My edited instructions.".to_string());
        storage
            .upsert_prompt_preset(&built_in)
            .expect("update built-in preset");
    }
    {
        let storage = Storage::open(&path).expect("reopen migrated database");
        let presets = storage
            .list_prompt_presets()
            .expect("list reopened presets");
        assert_eq!(presets.len(), 2);
        assert!(presets.contains(&custom));
        let built_in = presets
            .iter()
            .find(|preset| preset.id == PromptPreset::DEFAULT_ASSISTANT_ID)
            .expect("built-in preset");
        assert_eq!(built_in.name, "My renamed assistant");
        assert_eq!(
            built_in.system_prompt.as_deref(),
            Some("My edited instructions.")
        );
        assert_eq!(
            built_in.built_in_key.as_deref(),
            Some(PromptPreset::DEFAULT_ASSISTANT_KEY)
        );
    }
    std::fs::remove_file(path).expect("remove test database");
}
