use reqwest::Client;
use std::time::Duration;

use crate::provider::Provider;

/// A provider that was automatically detected on the local machine.
#[derive(Debug, Clone)]
pub struct DetectedProvider {
    /// Human-readable name (e.g. "Ollama", "LM Studio").
    pub name: String,
    /// Base URL where the provider is running.
    pub url: String,
    /// Number of models available.
    pub model_count: usize,
    /// Names of available models.
    pub models: Vec<String>,
    /// Ready-to-use provider instance.
    pub provider: Provider,
}

/// Scan the local machine for running LLM providers.
///
/// Checks Ollama (default + `OLLAMA_HOST`), LM Studio (default + `LM_STUDIO_URL`),
/// and any additional URLs you pass in `extra_urls`.
///
/// Returns only providers that are reachable and have at least one model loaded.
///
/// # Example
///
/// ```rust,no_run
/// use ai_assistant_core::detect;
///
/// # async fn example() -> Result<(), ai_assistant_core::Error> {
/// let providers = detect(&[]).await;
/// if providers.is_empty() {
///     println!("No LLM providers found. Install Ollama or LM Studio.");
/// } else {
///     for p in &providers {
///         println!("{} at {} ({} models)", p.name, p.url, p.model_count);
///     }
///     // Use the first detected provider
///     let reply = providers[0].provider.chat(&providers[0].models[0], "Hello!").await?;
///     println!("{reply}");
/// }
/// # Ok(())
/// # }
/// ```
pub async fn detect(extra_urls: &[&str]) -> Vec<DetectedProvider> {
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    let mut targets: Vec<(&str, &str)> = Vec::new();

    // Ollama
    let ollama_url = std::env::var("OLLAMA_HOST")
        .unwrap_or_else(|_| "http://localhost:11434".to_string());
    // Leak is fine here — runs once at startup
    let ollama_leaked: &str = Box::leak(ollama_url.into_boxed_str());
    targets.push(("Ollama", ollama_leaked));

    // LM Studio
    let lm_url = std::env::var("LM_STUDIO_URL")
        .unwrap_or_else(|_| "http://localhost:1234".to_string());
    let lm_leaked: &str = Box::leak(lm_url.into_boxed_str());
    targets.push(("LM Studio", lm_leaked));

    // Extra URLs
    for url in extra_urls {
        targets.push(("Custom", url));
    }

    let mut detected = Vec::new();

    for (name, base_url) in &targets {
        let base = base_url.trim_end_matches('/');

        // Try Ollama-style API first (/api/tags)
        if let Some(d) = try_ollama(&client, name, base).await {
            detected.push(d);
            continue;
        }

        // Try OpenAI-compatible API (/v1/models or /models)
        let oai_base = if base.ends_with("/v1") {
            base.to_string()
        } else {
            format!("{}/v1", base)
        };
        if let Some(d) = try_openai_compat(&client, name, base, &oai_base).await {
            detected.push(d);
        }
    }

    detected
}

async fn try_ollama(client: &Client, name: &str, base_url: &str) -> Option<DetectedProvider> {
    let url = format!("{}/api/tags", base_url);
    let resp = client.get(&url).send().await.ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    let models_arr = body.get("models")?.as_array()?;
    let models: Vec<String> = models_arr
        .iter()
        .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
        .collect();
    if models.is_empty() {
        return None;
    }
    Some(DetectedProvider {
        name: name.to_string(),
        url: base_url.to_string(),
        model_count: models.len(),
        models,
        provider: Provider::ollama(base_url),
    })
}

async fn try_openai_compat(
    client: &Client,
    name: &str,
    original_url: &str,
    oai_base: &str,
) -> Option<DetectedProvider> {
    let url = format!("{}/models", oai_base);
    let resp = client.get(&url).send().await.ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    let data = body.get("data")?.as_array()?;
    let models: Vec<String> = data
        .iter()
        .filter_map(|m| m.get("id").and_then(|n| n.as_str()).map(|s| s.to_string()))
        .collect();
    if models.is_empty() {
        return None;
    }
    Some(DetectedProvider {
        name: name.to_string(),
        url: original_url.to_string(),
        model_count: models.len(),
        models,
        provider: Provider::openai_compat(oai_base, name),
    })
}
