//! Expose your local LLM as an OpenAI-compatible API.
//!
//! Run with: cargo run --example serve_model --features serve
//!
//! Then test with:
//!   curl http://localhost:8090/v1/models
//!   curl -X POST http://localhost:8090/v1/chat/completions \
//!     -H "Content-Type: application/json" \
//!     -d '{"model":"llama3.2:1b","messages":[{"role":"user","content":"Hello!"}]}'

use ai_assistant_core::{detect, ProviderServiceBuilder};

#[tokio::main]
async fn main() -> Result<(), ai_assistant_core::Error> {
    // Auto-detect a local provider
    println!("Scanning for LLM providers...\n");
    let providers = detect(&[]).await;

    if providers.is_empty() {
        println!("No providers found! Install Ollama or LM Studio.");
        return Ok(());
    }

    let p = &providers[0];
    println!("Found: {} at {} ({} models)\n", p.name, p.url, p.model_count);

    // Serve it as an OpenAI-compatible API
    ProviderServiceBuilder::new(p.provider.clone())
        .port(8090)
        // .token("my_secret")    // uncomment to require auth
        // .nat()                 // uncomment for NAT traversal
        .start()
        .await?;

    Ok(())
}
