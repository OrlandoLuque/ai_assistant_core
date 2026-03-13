use serde::{Deserialize, Serialize};

/// Role in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// A chat message with a role and content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    /// Create a system message.
    pub fn system(content: &str) -> Self {
        Self { role: Role::System, content: content.to_string() }
    }

    /// Create a user message.
    pub fn user(content: &str) -> Self {
        Self { role: Role::User, content: content.to_string() }
    }

    /// Create an assistant message.
    pub fn assistant(content: &str) -> Self {
        Self { role: Role::Assistant, content: content.to_string() }
    }
}

/// Information about an available model.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Model name/identifier (e.g. `"llama3.2:1b"`, `"mistral:7b"`).
    pub name: String,
    /// Size in bytes (if reported by the provider).
    pub size: Option<u64>,
    /// Human-readable size string.
    pub size_display: String,
}

// --- Internal JSON structures for Ollama API ---

#[derive(Deserialize)]
pub(crate) struct OllamaModelsResponse {
    pub models: Option<Vec<OllamaModel>>,
}

#[derive(Deserialize)]
pub(crate) struct OllamaModel {
    pub name: String,
    pub size: Option<u64>,
}

#[derive(Serialize)]
pub(crate) struct OllamaChatRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [Message],
    pub stream: bool,
}

#[derive(Deserialize)]
pub(crate) struct OllamaChatResponse {
    pub message: Option<OllamaMessageContent>,
}

#[derive(Deserialize)]
pub(crate) struct OllamaMessageContent {
    pub content: String,
}

#[derive(Deserialize)]
pub(crate) struct OllamaStreamChunk {
    pub message: Option<OllamaMessageContent>,
    pub done: Option<bool>,
}

// --- Internal JSON structures for OpenAI-compatible API ---

#[derive(Deserialize)]
pub(crate) struct OpenAIModelsResponse {
    pub data: Option<Vec<OpenAIModel>>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAIModel {
    pub id: String,
}

#[derive(Serialize)]
pub(crate) struct OpenAIChatRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [Message],
    pub stream: bool,
}

#[derive(Deserialize)]
pub(crate) struct OpenAIChatResponse {
    pub choices: Option<Vec<OpenAIChoice>>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAIChoice {
    pub message: Option<OpenAIMessage>,
    pub delta: Option<OpenAIMessage>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAIMessage {
    pub content: Option<String>,
}
