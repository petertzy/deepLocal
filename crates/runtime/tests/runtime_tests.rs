use deeplocal_core::{
    ChatMessage, ChatRole, GenerationParameters, GenerationRequest, LoadOptions, ModelDescriptor,
};
use deeplocal_runtime::{MockBackend, RuntimeManager};
use futures::StreamExt;
use std::sync::Arc;

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
