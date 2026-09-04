use deeplocal_core::{ChatRole, ModelDescriptor};
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
    assert!(storage.list_chat_sessions().expect("list sessions").is_empty());
}
