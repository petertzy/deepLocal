//! Discovery of an optional, app-managed OPUS-MT Python runtime.
//!
//! The managed runtime is installed separately from model weights. Runtime
//! distribution and signed archive installation are intentionally not enabled
//! until verified platform-specific release artifacts are available.

use std::path::{Path, PathBuf};

pub(crate) fn managed_runtime_directory(models_root: &Path) -> PathBuf {
    models_root.join(".translation-runtime")
}

pub(crate) fn managed_python(models_root: &Path) -> PathBuf {
    let root = managed_runtime_directory(models_root);
    if cfg!(windows) {
        root.join("Scripts").join("python.exe")
    } else {
        root.join("bin").join("python")
    }
}

pub(crate) fn selected_python(models_root: &Path) -> (PathBuf, &'static str) {
    if let Some(explicit) = std::env::var_os("DEEPLOCAL_OPUS_MT_PYTHON") {
        if !explicit.is_empty() {
            return (PathBuf::from(explicit), "environment");
        }
    }
    let managed = managed_python(models_root);
    if managed.is_file() {
        return (managed, "managed");
    }
    (PathBuf::from("python3"), "system")
}

pub(crate) async fn check_dependencies(python: &Path) -> bool {
    matches!(
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio::process::Command::new(python)
                .arg("-c")
                .arg("import ctranslate2, sentencepiece")
                .kill_on_drop(true)
                .output()
        ).await,
        Ok(Ok(output)) if output.status.success()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_runtime_is_separate_from_model_directions() {
        let models = Path::new("/example/models");
        let runtime = managed_runtime_directory(models);
        assert_eq!(runtime, models.join(".translation-runtime"));
        assert_ne!(runtime, models.join("opus-mt/en-zh"));
        assert_ne!(runtime, models.join("opus-mt/zh-en"));
    }

    #[test]
    fn platform_interpreter_path_is_stable() {
        let python = managed_python(Path::new("/example/models"));
        if cfg!(windows) {
            assert!(python.ends_with("Scripts/python.exe"));
        } else {
            assert!(python.ends_with("bin/python"));
        }
    }
}
