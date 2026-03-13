use futures::stream::{self, Stream};
use reqwest::Client;
use std::pin::Pin;

use crate::error::Error;
use crate::types::*;

/// Backend kind.
#[derive(Debug, Clone)]
enum Backend {
    Ollama { base_url: String },
    OpenAICompat { base_url: String },
}

/// An LLM provider that can list models, chat, and stream responses.
#[derive(Debug, Clone)]
pub struct Provider {
    backend: Backend,
    client: Client,
}

impl Provider {
    /// Create an Ollama provider.
    pub(crate) fn ollama(base_url: &str) -> Self {
        Self {
            backend: Backend::Ollama { base_url: base_url.trim_end_matches('/').to_string() },
            client: Client::new(),
        }
    }

    /// Create an OpenAI-compatible provider.
    pub(crate) fn openai_compat(base_url: &str, _label: &str) -> Self {
        Self {
            backend: Backend::OpenAICompat {
                base_url: base_url.trim_end_matches('/').to_string(),
            },
            client: Client::new(),
        }
    }

    /// List available models from this provider.
    pub async fn models(&self) -> Result<Vec<ModelInfo>, Error> {
        match &self.backend {
            Backend::Ollama { base_url } => self.ollama_models(base_url).await,
            Backend::OpenAICompat { base_url } => self.openai_models(base_url).await,
        }
    }

    /// Send a single user message and return the complete response.
    pub async fn chat(&self, model: &str, user_message: &str) -> Result<String, Error> {
        let messages = [Message::user(user_message)];
        self.send(model, &messages).await
    }

    /// Send a conversation (message history) and return the complete response.
    pub async fn send(&self, model: &str, messages: &[Message]) -> Result<String, Error> {
        match &self.backend {
            Backend::Ollama { base_url } => self.ollama_chat(base_url, model, messages).await,
            Backend::OpenAICompat { base_url } => {
                self.openai_chat(base_url, model, messages).await
            }
        }
    }

    /// Stream a single user message, yielding response chunks as they arrive.
    pub async fn chat_stream(
        &self,
        model: &str,
        user_message: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, Error>> + Send>>, Error> {
        let messages = [Message::user(user_message)];
        self.send_stream(model, &messages).await
    }

    /// Stream a conversation, yielding response chunks as they arrive.
    pub async fn send_stream(
        &self,
        model: &str,
        messages: &[Message],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, Error>> + Send>>, Error> {
        match &self.backend {
            Backend::Ollama { base_url } => {
                self.ollama_stream(base_url, model, messages).await
            }
            Backend::OpenAICompat { base_url } => {
                self.openai_stream(base_url, model, messages).await
            }
        }
    }

    // ── Ollama implementation ──────────────────────────────────────────

    async fn ollama_models(&self, base_url: &str) -> Result<Vec<ModelInfo>, Error> {
        let url = format!("{}/api/tags", base_url);
        let resp: OllamaModelsResponse = self.client.get(&url).send().await?.json().await?;
        let models = resp.models.unwrap_or_default();
        Ok(models
            .into_iter()
            .map(|m| ModelInfo {
                size_display: format_size(m.size.unwrap_or(0)),
                name: m.name,
                size: m.size,
            })
            .collect())
    }

    async fn ollama_chat(
        &self,
        base_url: &str,
        model: &str,
        messages: &[Message],
    ) -> Result<String, Error> {
        let url = format!("{}/api/chat", base_url);
        let body = OllamaChatRequest { model, messages, stream: false };
        let resp: OllamaChatResponse =
            self.client.post(&url).json(&body).send().await?.json().await?;
        resp.message
            .map(|m| m.content)
            .ok_or_else(|| Error::Provider("Empty response from Ollama".into()))
    }

    async fn ollama_stream(
        &self,
        base_url: &str,
        model: &str,
        messages: &[Message],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, Error>> + Send>>, Error> {
        let url = format!("{}/api/chat", base_url);
        let body = OllamaChatRequest { model, messages, stream: true };
        let resp = self.client.post(&url).json(&body).send().await?;

        let byte_stream = resp.bytes_stream();
        let mapped = stream::unfold(
            (byte_stream, String::new()),
            |(mut byte_stream, mut buffer)| async move {
                use futures::StreamExt;
                loop {
                    // Try to extract a complete JSON line from buffer
                    if let Some(newline_pos) = buffer.find('\n') {
                        let line = buffer[..newline_pos].to_string();
                        buffer = buffer[newline_pos + 1..].to_string();
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<OllamaStreamChunk>(line) {
                            Ok(chunk) => {
                                if chunk.done.unwrap_or(false) {
                                    return None;
                                }
                                if let Some(msg) = chunk.message {
                                    if !msg.content.is_empty() {
                                        return Some((Ok(msg.content), (byte_stream, buffer)));
                                    }
                                }
                                continue;
                            }
                            Err(e) => {
                                return Some((Err(Error::Json(e)), (byte_stream, buffer)));
                            }
                        }
                    }

                    // Need more data
                    match byte_stream.next().await {
                        Some(Ok(bytes)) => {
                            buffer.push_str(&String::from_utf8_lossy(&bytes));
                        }
                        Some(Err(e)) => {
                            return Some((Err(Error::Http(e)), (byte_stream, buffer)));
                        }
                        None => return None,
                    }
                }
            },
        );
        Ok(Box::pin(mapped))
    }

    // ── OpenAI-compatible implementation ───────────────────────────────

    async fn openai_models(&self, base_url: &str) -> Result<Vec<ModelInfo>, Error> {
        let url = format!("{}/models", base_url);
        let resp: OpenAIModelsResponse = self.client.get(&url).send().await?.json().await?;
        let models = resp.data.unwrap_or_default();
        Ok(models
            .into_iter()
            .map(|m| ModelInfo {
                name: m.id,
                size: None,
                size_display: String::new(),
            })
            .collect())
    }

    async fn openai_chat(
        &self,
        base_url: &str,
        model: &str,
        messages: &[Message],
    ) -> Result<String, Error> {
        let url = format!("{}/chat/completions", base_url);
        let body = OpenAIChatRequest { model, messages, stream: false };
        let resp: OpenAIChatResponse =
            self.client.post(&url).json(&body).send().await?.json().await?;
        resp.choices
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.message)
            .and_then(|m| m.content)
            .ok_or_else(|| Error::Provider("Empty response".into()))
    }

    async fn openai_stream(
        &self,
        base_url: &str,
        model: &str,
        messages: &[Message],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, Error>> + Send>>, Error> {
        let url = format!("{}/chat/completions", base_url);
        let body = OpenAIChatRequest { model, messages, stream: true };
        let resp = self.client.post(&url).json(&body).send().await?;

        let byte_stream = resp.bytes_stream();
        let mapped = stream::unfold(
            (byte_stream, String::new()),
            |(mut byte_stream, mut buffer)| async move {
                use futures::StreamExt;
                loop {
                    if let Some(newline_pos) = buffer.find('\n') {
                        let line = buffer[..newline_pos].to_string();
                        buffer = buffer[newline_pos + 1..].to_string();
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        // SSE format: "data: {...}" or "data: [DONE]"
                        let data = line.strip_prefix("data: ").unwrap_or(line);
                        if data == "[DONE]" {
                            return None;
                        }
                        match serde_json::from_str::<OpenAIChatResponse>(data) {
                            Ok(resp) => {
                                if let Some(content) = resp
                                    .choices
                                    .and_then(|c| c.into_iter().next())
                                    .and_then(|c| c.delta)
                                    .and_then(|d| d.content)
                                {
                                    if !content.is_empty() {
                                        return Some((Ok(content), (byte_stream, buffer)));
                                    }
                                }
                                continue;
                            }
                            Err(_) => continue, // skip unparseable lines
                        }
                    }

                    match byte_stream.next().await {
                        Some(Ok(bytes)) => {
                            buffer.push_str(&String::from_utf8_lossy(&bytes));
                        }
                        Some(Err(e)) => {
                            return Some((Err(Error::Http(e)), (byte_stream, buffer)));
                        }
                        None => return None,
                    }
                }
            },
        );
        Ok(Box::pin(mapped))
    }
}

fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return String::new();
    }
    const GB: u64 = 1_073_741_824;
    const MB: u64 = 1_048_576;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else {
        format!("{:.0} MB", bytes as f64 / MB as f64)
    }
}
