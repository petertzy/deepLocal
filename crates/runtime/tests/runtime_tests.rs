use deeplocal_core::{
    ChatMessage, ChatRole, GenerationParameters, GenerationRequest, InferenceBackend, LoadOptions,
    ModelDescriptor,
};
use deeplocal_runtime::{LlamaCppBackend, MockBackend, RuntimeManager};
use futures::StreamExt;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[tokio::test]
async fn mock_backend_streams_tokens_for_loaded_model() {
    let runtime = RuntimeManager::default();
    runtime.register_backend(Arc::new(MockBackend)).await;
    runtime
        .load_model(
            "mock",
            ModelDescriptor::local_gguf("mock-model", "mock.gguf"),
            LoadOptions {
                context_length: Some(4096),
                gpu_layers: Some(0),
            },
        )
        .await
        .expect("load model");

    let mut stream = runtime
        .generate(GenerationRequest {
            model: "mock-model".to_string(),
            messages: vec![ChatMessage::new(ChatRole::User, "hello")],
            parameters: GenerationParameters::default(),
            stream: true,
        })
        .await
        .expect("generate");

    let mut output = String::new();
    while let Some(token) = stream.next().await {
        output.push_str(&token.expect("token").text);
    }

    assert!(output.contains("deepLocal mock response"));
    assert!(output.contains("hello"));
}

#[tokio::test]
async fn removes_registered_model_without_loading_it() {
    let runtime = RuntimeManager::default();
    runtime
        .register_model(ModelDescriptor::local_gguf("delete-me", "delete-me.gguf"))
        .await;

    assert!(runtime.get_model("delete-me").await.is_some());
    assert!(runtime.remove_model("delete-me").await.is_some());
    assert!(runtime.get_model("delete-me").await.is_none());
}

#[tokio::test]
async fn reports_backend_statuses() {
    let runtime = RuntimeManager::default();
    runtime.register_backend(Arc::new(MockBackend)).await;
    runtime
        .register_backend(Arc::new(LlamaCppBackend::new(
            "/definitely/missing/llama-server",
        )))
        .await;

    let statuses = runtime.list_backend_statuses().await;
    let mock = statuses
        .iter()
        .find(|status| status.id == "mock")
        .expect("mock status");
    let llama = statuses
        .iter()
        .find(|status| status.id == "llama.cpp")
        .expect("llama.cpp status");

    assert!(mock.available);
    assert!(!llama.available);
    assert_eq!(
        llama.binary_path.as_deref(),
        Some("/definitely/missing/llama-server")
    );
    assert!(
        llama
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("llama-server")
    );
}

#[tokio::test]
async fn failed_llama_load_does_not_leave_process_table_entry() {
    let binary = if PathBuf::from("/usr/bin/false").exists() {
        "/usr/bin/false"
    } else {
        "/bin/false"
    };
    let model_path = std::env::temp_dir().join(format!(
        "deeplocal-runtime-test-{}.gguf",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::write(&model_path, b"GGUF").expect("write model placeholder");
    let backend = LlamaCppBackend::new_for_tests(binary, 1);

    let result = backend
        .load(
            ModelDescriptor::local_gguf("failed-load", model_path.to_string_lossy().to_string()),
            LoadOptions {
                context_length: None,
                gpu_layers: None,
            },
        )
        .await;

    let _ = std::fs::remove_file(model_path);
    assert!(result.is_err());
    assert_eq!(backend.process_count().await, 0);
}
