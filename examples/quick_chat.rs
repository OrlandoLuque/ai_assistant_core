use ai_assistant_core::{detect, Message};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), ai_assistant_core::Error> {
    // 1. Auto-detect local providers
    println!("Scanning for LLM providers...\n");
    let providers = detect(&[]).await;

    if providers.is_empty() {
        println!("No providers found! Install Ollama or LM Studio.");
        return Ok(());
    }

    for p in &providers {
        println!("  Found: {} at {} ({} models)", p.name, p.url, p.model_count);
        for m in &p.models {
            println!("    - {}", m);
        }
    }

    // 2. Use the first provider and its first model
    let p = &providers[0];
    let model = &p.models[0];
    println!("\nUsing {} / {}\n", p.name, model);

    // 3. Simple chat
    println!("--- Simple chat ---");
    let reply = p.provider.chat(model, "What is Rust in one sentence?").await?;
    println!("Reply: {}\n", reply);

    // 4. Chat with message history
    println!("--- Conversation ---");
    let messages = vec![
        Message::system("You are a helpful assistant. Be concise."),
        Message::user("What are the 3 main features of Rust?"),
    ];
    let reply = p.provider.send(model, &messages).await?;
    println!("Reply: {}\n", reply);

    // 5. Streaming
    println!("--- Streaming ---");
    let mut stream = p.provider.chat_stream(model, "Count from 1 to 5, one per line.").await?;
    while let Some(chunk) = stream.next().await {
        print!("{}", chunk?);
    }
    println!("\n\n--- Done! ---");

    Ok(())
}
