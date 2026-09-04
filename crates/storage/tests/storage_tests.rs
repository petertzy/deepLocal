use chrono::Utc;
use deeplocal_core::{ChatRole, DownloadJob, ModelDescriptor};
use deeplocal_storage::Storage;

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
