# ai_assistant_core

Simple, ergonomic Rust client for local LLMs.

Connect to **Ollama**, **LM Studio**, or any **OpenAI-compatible** server in a few lines of code.

## Quick Start

```toml
[dependencies]
ai_assistant_core = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

```rust
use ai_assistant_core::ollama;

#[tokio::main]
async fn main() -> Result<(), ai_assistant_core::Error> {
    let provider = ollama();

    // List models
    let models = provider.models().await?;
    for m in &models {
        println!("{} ({})", m.name, m.size_display);
    }

    // Chat
    let reply = provider.chat("llama3.2:1b", "What is Rust?").await?;
    println!("{reply}");

    Ok(())
}
```

## Features

- **Multi-provider**: Ollama, LM Studio, any OpenAI-compatible API
- **Streaming**: Token-by-token responses via `futures::Stream`
- **Message history**: Full conversation support with system/user/assistant roles
- **Minimal dependencies**: Just `reqwest`, `serde`, `tokio`, `futures`, `thiserror`
- **MIT / Apache-2.0**: Use it anywhere

## Providers

```rust
use ai_assistant_core::{ollama, ollama_at, lm_studio, openai_compat};

let o  = ollama();                                        // localhost:11434
let o2 = ollama_at("http://192.168.1.50:11434");          // remote Ollama
let lm = lm_studio();                                    // localhost:1234
let c  = openai_compat("http://localhost:8080/v1");       // any compatible API
```

## Streaming

```rust
use ai_assistant_core::ollama;
use futures::StreamExt;

# async fn example() -> Result<(), ai_assistant_core::Error> {
let provider = ollama();
let mut stream = provider.chat_stream("llama3.2:1b", "Tell me a joke").await?;
while let Some(chunk) = stream.next().await {
    print!("{}", chunk?);
}
# Ok(())
# }
```

## Conversation History

```rust
use ai_assistant_core::{ollama, Message};

# async fn example() -> Result<(), ai_assistant_core::Error> {
let provider = ollama();
let messages = vec![
    Message::system("You are a pirate. Answer everything like a pirate."),
    Message::user("What is the weather like?"),
];
let reply = provider.send("llama3.2:1b", &messages).await?;
println!("{reply}");
# Ok(())
# }
```

## Need More?

For advanced features like RAG, multi-agent orchestration, security guardrails,
distributed clusters, MCP protocol, autonomous agents, and more, check out the
full [ai_assistant](https://github.com/OrlandoLuque/ai_assistant) suite.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
