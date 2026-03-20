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
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt; // for oneshot

    #[test]
    fn test_serve_config_defaults() {
        let config = ServeConfig::default();
        assert_eq!(config.port, 8090);
        assert_eq!(config.host, "0.0.0.0");
        assert!(config.auth_token.is_none());
        assert!(config.cors);
    }

    /// Start a mock Ollama backend that returns fixed responses.
    async fn start_mock_backend() -> (u16, tokio::task::JoinHandle<()>) {
        let app = axum::Router::new()
            .route("/api/tags", axum::routing::get(|| async {
                axum::Json(serde_json::json!({
                    "models": [
                        {"name": "test-model", "size": 1000000},
                        {"name": "test-model-2", "size": 2000000}
                    ]
                }))
            }))
            .route("/api/chat", axum::routing::post(|axum::Json(body): axum::Json<serde_json::Value>| async move {
                let stream = body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
                if stream {
                    let chunks = vec![
                        r#"{"message":{"content":"Hello"},"done":false}"#,
                        r#"{"message":{"content":" world"},"done":false}"#,
                        r#"{"done":true}"#,
                    ];
                    let body = chunks.join("\n") + "\n";
                    axum::response::Response::builder()
                        .header("content-type", "application/x-ndjson")
                        .body(Body::from(body))
                        .unwrap()
                } else {
                    axum::Json(serde_json::json!({
                        "message": {"role": "assistant", "content": "Mock response from test backend"}
                    })).into_response()
                }
            }));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        // Give it a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        (port, handle)
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let provider = crate::ollama(); // won't be called for /health
        let config = ServeConfig::default();
        let app = build_router(provider, &config);

        let resp = app
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn test_auth_rejected_without_token() {
        let provider = crate::ollama();
        let mut config = ServeConfig::default();
        config.auth_token = Some("secret123".to_string());
        let app = build_router(provider, &config);

        // No auth header → 401
        let resp = app
            .oneshot(Request::get("/v1/models").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), 401);
    }

    #[tokio::test]
    async fn test_auth_accepted_with_correct_token() {
        let (mock_port, _handle) = start_mock_backend().await;

        let provider = crate::ollama_at(&format!("http://127.0.0.1:{}", mock_port));
        let mut config = ServeConfig::default();
        config.auth_token = Some("secret123".to_string());
        let app = build_router(provider, &config);

        let resp = app
            .oneshot(
                Request::get("/v1/models")
                    .header("Authorization", "Bearer secret123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["object"], "list");
        let data = json["data"].as_array().unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0]["id"], "test-model");
    }

    #[tokio::test]
    async fn test_proxy_models() {
        let (mock_port, _handle) = start_mock_backend().await;

        let provider = crate::ollama_at(&format!("http://127.0.0.1:{}", mock_port));
        let config = ServeConfig::default(); // no auth
        let app = build_router(provider, &config);

        let resp = app
            .oneshot(Request::get("/v1/models").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let data = json["data"].as_array().unwrap();
        assert_eq!(data.len(), 2);
    }

    #[tokio::test]
    async fn test_proxy_chat_non_streaming() {
        let (mock_port, _handle) = start_mock_backend().await;

        let provider = crate::ollama_at(&format!("http://127.0.0.1:{}", mock_port));
        let config = ServeConfig::default();
        let app = build_router(provider, &config);

        let body = serde_json::json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let resp = app
            .oneshot(
                Request::post("/v1/chat/completions")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["object"], "chat.completion");
        let content = json["choices"][0]["message"]["content"].as_str().unwrap();
        assert_eq!(content, "Mock response from test backend");
    }

    #[tokio::test]
    async fn test_proxy_chat_streaming() {
        let (mock_port, _handle) = start_mock_backend().await;

        let provider = crate::ollama_at(&format!("http://127.0.0.1:{}", mock_port));
        let config = ServeConfig::default();
        let app = build_router(provider, &config);

        let body = serde_json::json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "Hello"}],
            "stream": true
        });

        let resp = app
            .oneshot(
                Request::post("/v1/chat/completions")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        // SSE responses have text/event-stream content type
        let content_type = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(content_type.contains("text/event-stream"), "Expected SSE, got: {}", content_type);

        // Read all SSE data
        let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        // Should contain the streamed chunks
        assert!(text.contains("Hello"), "SSE should contain 'Hello', got: {}", text);
        assert!(text.contains("world"), "SSE should contain 'world', got: {}", text);
    }

    #[tokio::test]
    async fn test_auth_wrong_token_rejected() {
        let provider = crate::ollama();
        let mut config = ServeConfig::default();
        config.auth_token = Some("correct_token".to_string());
        let app = build_router(provider, &config);

        let resp = app
            .oneshot(
                Request::get("/v1/models")
                    .header("Authorization", "Bearer wrong_token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), 401);
    }

    #[tokio::test]
    async fn test_no_auth_when_not_configured() {
        let (mock_port, _handle) = start_mock_backend().await;

        let provider = crate::ollama_at(&format!("http://127.0.0.1:{}", mock_port));
        let config = ServeConfig::default(); // no token = no auth
        let app = build_router(provider, &config);

        // Should work without any auth header
        let resp = app
            .oneshot(Request::get("/v1/models").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
    }
}
