//! Unified provider service: server + NAT traversal.
//!
//! Combines the HTTP server with optional NAT traversal to expose
//! a local LLM provider as a remotely accessible OpenAI-compatible API.

use crate::nat::{NatConfig, NatResult};
use crate::provider::Provider;
use crate::server::{build_router, ServeConfig};

/// Information about a running provider service.
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    /// Local URL where the server is listening.
    pub local_url: String,
    /// Public URL (if NAT traversal succeeded).
    pub public_url: Option<String>,
    /// Available models from the backend.
    pub models: Vec<String>,
    /// NAT traversal result (if enabled).
    pub nat: Option<NatResult>,
}

/// Builder for configuring and starting a provider service.
pub struct ProviderServiceBuilder {
    provider: Provider,
    server_config: ServeConfig,
    nat_config: Option<NatConfig>,
}

impl ProviderServiceBuilder {
    /// Create a new builder with a provider backend.
    pub fn new(provider: Provider) -> Self {
        Self {
            provider,
            server_config: ServeConfig::default(),
            nat_config: None,
        }
    }

    /// Set the server port.
    pub fn port(mut self, port: u16) -> Self {
        self.server_config.port = port;
        self
    }

    /// Set the bind host.
    pub fn host(mut self, host: &str) -> Self {
        self.server_config.host = host.to_string();
        self
    }

    /// Require a bearer token for authentication.
    pub fn token(mut self, token: &str) -> Self {
        self.server_config.auth_token = Some(token.to_string());
        self
    }

    /// Set full server configuration.
    pub fn server_config(mut self, config: ServeConfig) -> Self {
        self.server_config = config;
        self
    }

    /// Enable NAT traversal with default settings.
    pub fn nat(mut self) -> Self {
        self.nat_config = Some(NatConfig::default());
        self
    }

    /// Enable NAT traversal with custom configuration.
    pub fn nat_config(mut self, config: NatConfig) -> Self {
        self.nat_config = Some(config);
        self
    }

    /// Start the provider service.
    ///
    /// This will:
    /// 1. Fetch available models from the backend
    /// 2. Optionally discover public IP and open ports via NAT traversal
    /// 3. Start the HTTP server
    /// 4. Block until the server is shut down (Ctrl+C)
    pub async fn start(self) -> Result<ServiceInfo, crate::Error> {
        let local_url = format!("http://{}:{}", self.server_config.host, self.server_config.port);

        // Fetch models from backend
        let models: Vec<String> = match self.provider.models().await {
            Ok(m) => m.into_iter().map(|m| m.name).collect(),
            Err(e) => {
                eprintln!("Warning: could not fetch models from backend: {}", e);
                Vec::new()
            }
        };

        // NAT traversal (if enabled)
        let nat = if let Some(ref nat_config) = self.nat_config {
            let result = crate::nat::discover_and_map(nat_config, self.server_config.port).await;
            Some(result)
        } else {
            None
        };

        let public_url = nat.as_ref().and_then(|n| n.public_url.clone());

        let info = ServiceInfo {
            local_url: local_url.clone(),
            public_url,
            models,
            nat,
        };

        // Print service info
        println!("ai_assistant_core provider service");
        println!("──────────────────────────────────");
        println!("  Local:    {}", info.local_url);
        if let Some(ref pub_url) = info.public_url {
            println!("  Public:   {}", pub_url);
        }
        if !info.models.is_empty() {
            println!("  Models:   {}", info.models.join(", "));
        }
        if let Some(ref nat) = info.nat {
            println!("  NAT type: {}", nat.nat_type);
            if nat.upnp_success {
                println!("  UPnP:     port mapped");
            }
            if nat.nat_pmp_success {
                println!("  NAT-PMP:  port mapped");
            }
        }
        if info.public_url.is_none() && self.nat_config.is_some() {
            println!("  Note:     NAT traversal attempted but no public URL available");
        }
        println!("──────────────────────────────────");
        println!("  Endpoints:");
        println!("    GET  /health");
        println!("    GET  /v1/models");
        println!("    POST /v1/chat/completions");
        if self.server_config.auth_token.is_some() {
            println!("  Auth:     Bearer token required");
        }
        println!();

        // Build and start server
        let router = build_router(self.provider, &self.server_config);
        let addr = format!("{}:{}", self.server_config.host, self.server_config.port);
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| crate::Error::Provider(format!("Failed to bind {}: {}", addr, e)))?;

        println!("Listening on {} — press Ctrl+C to stop", addr);

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .map_err(|e| crate::Error::Provider(format!("Server error: {}", e)))?;

        println!("\nServer stopped.");
        Ok(info)
    }
}

/// Wait for Ctrl+C signal.
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
}

/// Convenience function: serve a provider with default settings.
///
/// ```rust,no_run
/// use ai_assistant_core::{ollama, serve};
///
/// #[tokio::main]
/// async fn main() {
///     let provider = ollama();
///     serve::quick(provider).await.unwrap();
/// }
/// ```
pub async fn quick(provider: Provider) -> Result<ServiceInfo, crate::Error> {
    ProviderServiceBuilder::new(provider).start().await
}
