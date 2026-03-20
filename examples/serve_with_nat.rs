//! Serve your local LLM with NAT traversal for remote access.
//!
//! Run with: cargo run --example serve_with_nat --features "serve,nat"
//!
//! This will:
//! 1. Auto-detect your local LLM (Ollama, LM Studio, etc.)
//! 2. Discover your public IP via STUN
//! 3. Attempt to open a port via UPnP or NAT-PMP
//! 4. Start an OpenAI-compatible server accessible remotely

use ai_assistant_core::{detect, ProviderServiceBuilder};

#[tokio::main]
async fn main() -> Result<(), ai_assistant_core::Error> {
    let providers = detect(&[]).await;

    if providers.is_empty() {
        println!("No providers found! Install Ollama or LM Studio.");
        return Ok(());
    }

    let p = &providers[0];
    println!("Serving {} ({} models) with NAT traversal...\n", p.name, p.model_count);

    ProviderServiceBuilder::new(p.provider.clone())
        .port(8090)
        .token("change_me_in_production")
        .nat() // enable STUN + UPnP + NAT-PMP
        .start()
        .await?;

    Ok(())
}
