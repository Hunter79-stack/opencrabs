//! Placeholder Provider
//!
//! A stub provider used when no real provider is configured.
//! Allows the app to start and show onboarding.

use async_trait::async_trait;

use crate::brain::provider::{LLMRequest, LLMResponse, Provider, ProviderStream, Result};

/// A placeholder provider that returns an error when used.
/// Used when no real provider is configured so the app can start and show onboarding.
pub struct PlaceholderProvider;

#[async_trait]
impl Provider for PlaceholderProvider {
    fn name(&self) -> &str {
        "none"
    }

    fn default_model(&self) -> &str {
        "none"
    }

    fn supported_models(&self) -> Vec<String> {
        vec![]
    }

    fn context_window(&self, _model: &str) -> Option<u32> {
        None
    }

    fn calculate_cost(&self, _model: &str, _input_tokens: u32, _output_tokens: u32) -> f64 {
        0.0
    }

    async fn complete(&self, _request: LLMRequest) -> Result<LLMResponse> {
        Err(crate::brain::provider::ProviderError::Internal(
            "No provider configured. Please complete onboarding to set up a provider.".to_string(),
        ))
    }

    async fn stream(&self, _request: LLMRequest) -> Result<ProviderStream> {
        Err(crate::brain::provider::ProviderError::Internal(
            "No provider configured. Please complete onboarding to set up a provider.".to_string(),
        ))
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn supports_tools(&self) -> bool {
        false
    }

    fn supports_vision(&self) -> bool {
        false
    }
}
