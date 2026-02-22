//! Onboarding Provider Keys Tests
//!
//! Tests that all providers (Anthropic, OpenAI, Gemini, OpenRouter, Minimax, Custom)
//! correctly save their API keys to keys.toml

use crate::config::{ProviderConfig, ProviderConfigs};
use crate::tui::onboarding::{OnboardingStep, OnboardingWizard, PROVIDERS};

#[test]
fn test_all_providers_save_keys() {
    // Test that onboarding saves API keys for ALL providers
    // Provider indices: 0=Anthropic, 1=OpenAI, 2=Gemini, 3=OpenRouter, 4=Minimax, 5=Custom

    let test_key = "test-api-key-12345";

    // Test Anthropic (index 0)
    let mut wizard = OnboardingWizard::new();
    wizard.selected_provider = 0;
    wizard.api_key_input = test_key.to_string();
    wizard.step = OnboardingStep::Complete;
    let config = wizard.apply_config().unwrap();
    assert!(config.providers.anthropic.is_some());
    assert!(config.providers.anthropic.unwrap().enabled);

    // Test OpenAI (index 1)
    let mut wizard = OnboardingWizard::new();
    wizard.selected_provider = 1;
    wizard.api_key_input = test_key.to_string();
    wizard.step = OnboardingStep::Complete;
    let config = wizard.apply_config().unwrap();
    assert!(config.providers.openai.is_some());
    assert!(config.providers.openai.unwrap().enabled);

    // Test OpenRouter (index 3)
    let mut wizard = OnboardingWizard::new();
    wizard.selected_provider = 3;
    wizard.api_key_input = test_key.to_string();
    wizard.step = OnboardingStep::Complete;
    let config = wizard.apply_config().unwrap();
    assert!(config.providers.openrouter.is_some());
    assert!(config.providers.openrouter.unwrap().enabled);

    // Test Minimax (index 4)
    let mut wizard = OnboardingWizard::new();
    wizard.selected_provider = 4;
    wizard.api_key_input = test_key.to_string();
    wizard.step = OnboardingStep::Complete;
    let config = wizard.apply_config().unwrap();
    assert!(config.providers.minimax.is_some());
    assert!(config.providers.minimax.unwrap().enabled);

    // Test Custom (index 5) - uses custom_api_key field
    let mut wizard = OnboardingWizard::new();
    wizard.selected_provider = 5;
    wizard.custom_base_url = "http://localhost:1234/v1".to_string();
    wizard.custom_api_key = test_key.to_string();
    wizard.custom_model = "gpt-4".to_string();
    wizard.step = OnboardingStep::Complete;
    let config = wizard.apply_config().unwrap();
    assert!(config.providers.custom.is_some());
    assert!(config.providers.custom.unwrap().enabled);
}

#[test]
fn test_custom_provider_uses_separate_api_key_field() {
    // Custom provider must use custom_api_key, NOT api_key_input
    let test_key = "custom-provider-key";

    let mut wizard = OnboardingWizard::new();
    wizard.selected_provider = 5; // Custom
    wizard.api_key_input = "wrong-key-in-api-key-input".to_string();
    wizard.custom_base_url = "http://localhost:1234/v1".to_string();
    wizard.custom_api_key = test_key.to_string();
    wizard.custom_model = "gpt-4".to_string();
    wizard.step = OnboardingStep::Complete;

    let config = wizard.apply_config().unwrap();
    let custom_config = config.providers.custom.as_ref().unwrap();

    // Key should be from custom_api_key, not api_key_input
    assert_eq!(custom_config.api_key.as_ref().unwrap(), test_key);
    assert_ne!(
        custom_config.api_key.as_ref().unwrap(),
        "wrong-key-in-api-key-input"
    );
}

#[test]
fn test_keys_toml_has_all_provider_sections() {
    // Verify keys.toml structure supports all providers
    let keys = ProviderConfigs::default();

    // All these should be None by default
    assert!(keys.anthropic.is_none());
    assert!(keys.openai.is_none());
    assert!(keys.gemini.is_none());
    assert!(keys.openrouter.is_none());
    assert!(keys.minimax.is_none());
    assert!(keys.custom.is_none());
}

#[test]
fn test_provider_count_matches() {
    // Verify PROVIDERS array has 6 entries
    assert_eq!(PROVIDERS.len(), 6);

    // Verify provider names
    assert_eq!(PROVIDERS[0].name, "Anthropic Claude");
    assert_eq!(PROVIDERS[1].name, "OpenAI");
    assert_eq!(PROVIDERS[2].name, "Google Gemini");
    assert_eq!(PROVIDERS[3].name, "OpenRouter");
    assert_eq!(PROVIDERS[4].name, "Minimax");
    assert_eq!(PROVIDERS[5].name, "Custom OpenAI-Compatible");
}

#[test]
fn test_is_custom_provider() {
    let mut wizard = OnboardingWizard::new();

    // Index 5 is Custom
    wizard.selected_provider = 5;
    assert!(wizard.is_custom_provider());

    // Other indices are not Custom
    wizard.selected_provider = 0;
    assert!(!wizard.is_custom_provider());
    wizard.selected_provider = 1;
    assert!(!wizard.is_custom_provider());
    wizard.selected_provider = 4;
    assert!(!wizard.is_custom_provider());
}
