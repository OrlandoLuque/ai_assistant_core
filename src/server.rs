//! Minimal OpenAI-compatible HTTP server that proxies to a local LLM provider.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use futures::StreamExt;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::provider::Provider;
use crate::types::Message;

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServeConfig {
    /// Port to listen on (default: 8090).
    pub port: u16,
    /// Host to bind to (default: 0.0.0.0).
    pub host: String,
    /// Optional bearer token for authentication.
    pub auth_token: Option<String>,
    /// Enable CORS (default: true).
    pub cors: bool,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            port: 8090,
            host: "0.0.0.0".to_string(),
            auth_token: None,
            cors: true,
        }
    }
}

/// Shared application state.
#[derive(Clone)]
pub(crate) struct AppState {
    provider: Arc<Mutex<Provider>>,
    auth_token: Option<String>,
}

/// Build the axum router with all endpoints.
pub(crate) fn build_router(provider: Provider, config: &ServeConfig) -> Router {
    let state = AppState {
        provider: Arc::new(Mutex::new(provider)),
        auth_token: config.auth_token.clone(),
    };

    let mut app = Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(state);

    if config.cors {
        use tower_http::cors::CorsLayer;
        app = app.layer(CorsLayer::permissive());
    }

    app
}

// ── Handlers ────────────────────────────────────────────────────────────────

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

async fn list_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;

    let provider = state.provider.lock().await;
    let models = provider.models().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    let data: Vec<serde_json::Value> = models
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.name,
                "object": "model",
                "owned_by": "local",
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "object": "list",
        "data": data,
    })))
}

async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ChatRequest>,
) -> Result<axum::response::Response, StatusCode> {
    check_auth(&state, &headers)?;

    let messages: Vec<Message> = req
        .messages
        .iter()
        .map(|m| Message {
            role: match m.role.as_str() {
                "system" => crate::types::Role::System,
                "assistant" => crate::types::Role::Assistant,
                _ => crate::types::Role::User,
            },
            content: m.content.clone(),
        })
        .collect();

    let provider = state.provider.lock().await;

    if req.stream.unwrap_or(false) {
        // Streaming response (SSE)
        let stream = provider
            .send_stream(&req.model, &messages)
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;

        let model = req.model.clone();
        let sse_stream = stream.map(move |chunk| {
            let content = chunk.unwrap_or_default();
            let data = serde_json::json!({
                "id": "chatcmpl-serve",
                "object": "chat.completion.chunk",
                "model": model,
                "choices": [{
                    "index": 0,
                    "delta": { "content": content },
                    "finish_reason": null
                }]
            });
            Ok::<_, Infallible>(Event::default().data(data.to_string()))
        });

        Ok(Sse::new(sse_stream).into_response())
    } else {
        // Non-streaming response
        let reply = provider
            .send(&req.model, &messages)
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;

        Ok(Json(serde_json::json!({
            "id": "chatcmpl-serve",
            "object": "chat.completion",
            "model": req.model,
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": reply,
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 0,
                "completion_tokens": 0,
                "total_tokens": 0
            }
        }))
        .into_response())
    }
}

fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    if let Some(ref token) = state.auth_token {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let provided = auth
            .strip_prefix("Bearer ")
            .or_else(|| auth.strip_prefix("bearer "))
            .unwrap_or("");

        if provided != token {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(())
}

// ── Request/Response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serve_config_defaults() {
        let config = ServeConfig::default();
        assert_eq!(config.port, 8090);
        assert_eq!(config.host, "0.0.0.0");
        assert!(config.auth_token.is_none());
        assert!(config.cors);
    }
}
