use deeplocal_core::ModelDescriptor;
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
