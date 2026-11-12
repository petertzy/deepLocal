//! Language-pair selection for optional, direction-specific translation models.
//!
//! This module is independent of the GGUF chat runtime. It does not load models,
//! download files, or silently route text to another provider.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationPair {
    pub source: String,
    pub target: String,
}

impl TranslationPair {
    pub fn new(source: &str, target: &str) -> Result<Self, &'static str> {
        let source = normalize_language(source)?;
        let target = normalize_language(target)?;
        if source == target {
            return Err("source and target languages must differ");
        }
        Ok(Self { source, target })
    }

    pub fn model_key(&self) -> String {
        format!("{}-{}", self.source, self.target)
    }
}

/// Normalize locale tags to the base language for direction-specific models.
///
/// Script-specific translations must be handled separately before using this
/// helper: for example, this function does not distinguish Chinese scripts.
fn normalize_language(value: &str) -> Result<String, &'static str> {
    let base = value.trim().split(['-', '_']).next().unwrap_or("");
    if !(2..=3).contains(&base.len()) || !base.bytes().all(|c| c.is_ascii_alphabetic()) {
        return Err("language must be an ISO 639 language code or locale");
    }
    Ok(base.to_ascii_lowercase())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationRoute {
    LocalModel { model_id: String },
    ExistingProviderFallback,
    Unavailable,
}

/// Choose a local translation model only when the exact direction is installed.
///
/// The caller must explicitly enable fallback. This function never selects a
/// remote provider and never sends text over the network.
pub fn select_route(
    pair: &TranslationPair,
    installed_models: &HashMap<String, String>,
    allow_existing_provider_fallback: bool,
) -> TranslationRoute {
    match installed_models.get(&pair.model_key()) {
        Some(model_id) => TranslationRoute::LocalModel {
            model_id: model_id.clone(),
        },
        None if allow_existing_provider_fallback => TranslationRoute::ExistingProviderFallback,
        None => TranslationRoute::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_normalization_selects_the_correct_direction() {
        let pair = TranslationPair::new("EN-us", "zh-CN").unwrap();
        assert_eq!(pair.model_key(), "en-zh");
        let reverse = TranslationPair::new("zh", "en").unwrap();
        assert_eq!(reverse.model_key(), "zh-en");
    }

    #[test]
    fn model_direction_is_not_reversible() {
        let installed = HashMap::from([(
            "en-zh".to_owned(),
            "opus-mt-en-zh".to_owned(),
        )]);
        let forward = TranslationPair::new("en", "zh").unwrap();
        let reverse = TranslationPair::new("zh", "en").unwrap();
        assert_eq!(
            select_route(&forward, &installed, false),
            TranslationRoute::LocalModel {
                model_id: "opus-mt-en-zh".to_owned()
            }
        );
        assert_eq!(
            select_route(&reverse, &installed, false),
            TranslationRoute::Unavailable
        );
    }

    #[test]
    fn fallback_requires_explicit_permission() {
        let pair = TranslationPair::new("de", "zh").unwrap();
        let installed = HashMap::new();
        assert_eq!(select_route(&pair, &installed, false), TranslationRoute::Unavailable);
        assert_eq!(
            select_route(&pair, &installed, true),
            TranslationRoute::ExistingProviderFallback
        );
    }

    #[test]
    fn invalid_or_identical_languages_are_rejected() {
        assert!(TranslationPair::new("en", "en-US").is_err());
        assert!(TranslationPair::new("../", "zh").is_err());
        assert!(TranslationPair::new("auto", "zh").is_err());
    }
}
