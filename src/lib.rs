//! # ai_assistant_core
//!
//! Simple, ergonomic Rust client for local LLMs.
//!
//! Connect to **Ollama**, **LM Studio**, or any **OpenAI-compatible** server
//! in a few lines of code. List models, chat, and stream responses.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use ai_assistant_core::{ollama, Message};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), ai_assistant_core::Error> {
//!     let provider = ollama();
//!
//!     // List available models
//!     let models = provider.models().await?;
//!     println!("Models: {:?}", models.iter().map(|m| &m.name).collect::<Vec<_>>());
//!
//!     // Simple chat
//!     let reply = provider.chat("llama3.2:1b", "What is Rust?").await?;
//!     println!("{reply}");
//!
//!     // Chat with message history
//!     let messages = vec![
//!         Message::system("You are a helpful assistant."),
//!         Message::user("Explain ownership in Rust in 2 sentences."),
//!     ];
//!     let reply = provider.send("llama3.2:1b", &messages).await?;
//!     println!("{reply}");
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Streaming
//!
//! ```rust,no_run
//! use ai_assistant_core::ollama;
//! use futures::StreamExt;
//!
//! # async fn example() -> Result<(), ai_assistant_core::Error> {
//! let provider = ollama();
//! let mut stream = provider.chat_stream("llama3.2:1b", "Tell me a joke").await?;
//! while let Some(chunk) = stream.next().await {
//!     print!("{}", chunk?);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Providers
//!
//! ```rust,no_run
//! use ai_assistant_core::{ollama, ollama_at, lm_studio, openai_compat};
//!
//! let o = ollama();                                         // localhost:11434
//! let o2 = ollama_at("http://192.168.1.50:11434");          // remote Ollama
//! let lm = lm_studio();                                    // localhost:1234
//! let custom = openai_compat("http://localhost:8080/v1");   // any OpenAI-compatible
//! ```
//!
//! ## Auto-detection
//!
//! ```rust,no_run
//! use ai_assistant_core::detect;
//!
//! # async fn example() -> Result<(), ai_assistant_core::Error> {
//! let providers = detect(&[]).await;
//! for p in &providers {
//!     println!("{} at {} ({} models)", p.name, p.url, p.model_count);
//! }
//! // Chat with the first available provider and model
//! if let Some(p) = providers.first() {
//!     let reply = p.provider.chat(&p.models[0], "Hello!").await?;
//!     println!("{reply}");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Need more?
//!
//! For advanced features (RAG, multi-agent, security, distributed clusters, MCP,
//! autonomous agents, and more), check out the full
//! [ai_assistant](https://github.com/OrlandoLuque/ai_assistant) suite.

mod detect_providers;
mod error;
mod provider;
mod types;

#[cfg(feature = "nat")]
pub mod nat;

#[cfg(feature = "serve")]
mod server;

#[cfg(feature = "serve")]
pub mod serve;

pub use detect_providers::{detect, DetectedProvider};
pub use error::Error;
pub use provider::Provider;
pub use types::{Message, ModelInfo, Role};

#[cfg(feature = "serve")]
pub use serve::{ProviderServiceBuilder, ServiceInfo};

#[cfg(feature = "nat")]
pub use nat::{NatConfig, NatResult};

/// Create an Ollama provider pointing to `http://localhost:11434`.
pub fn ollama() -> Provider {
    Provider::ollama("http://localhost:11434")
}

/// Create an Ollama provider at a custom URL.
pub fn ollama_at(base_url: &str) -> Provider {
    Provider::ollama(base_url)
}

/// Create an LM Studio provider pointing to `http://localhost:1234/v1`.
pub fn lm_studio() -> Provider {
    Provider::openai_compat("http://localhost:1234/v1", "LM Studio")
}

/// Create a provider for any OpenAI-compatible API.
pub fn openai_compat(base_url: &str) -> Provider {
    Provider::openai_compat(base_url, "OpenAI-compatible")
}
