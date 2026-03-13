/// Errors returned by `ai_assistant_core`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization/deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The LLM provider returned an error message.
    #[error("Provider error: {0}")]
    Provider(String),

    /// No models available from the provider.
    #[error("No models available — is the provider running?")]
    NoModels,
}
