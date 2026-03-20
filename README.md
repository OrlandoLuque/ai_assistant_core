# ai_assistant_core

Simple, ergonomic Rust client **and server** for local LLMs.

Connect to **Ollama**, **LM Studio**, or any **OpenAI-compatible** server in a few lines of code. Or **serve your local model** as an OpenAI-compatible API accessible remotely.

## Quick Start

```toml
[dependencies]
ai_assistant_core = "0.2"
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

- **Auto-detection**: Scans localhost for running providers automatically
- **Multi-provider**: Ollama, LM Studio, any OpenAI-compatible API
- **Streaming**: Token-by-token responses via `futures::Stream`
- **Message history**: Full conversation support with system/user/assistant roles
- **Serve mode**: Expose your local LLM as an OpenAI-compatible HTTP API
- **NAT traversal**: STUN discovery + UPnP/NAT-PMP port mapping for remote access
- **Standalone binary**: `ai_serve` — one command to share your model
- **Minimal dependencies**: Lightweight, fast to compile
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

## Auto-Detection

Don't know what's running? Let `detect()` find providers for you:

```rust
use ai_assistant_core::detect;

# async fn example() -> Result<(), ai_assistant_core::Error> {
let providers = detect(&[]).await;
for p in &providers {
    println!("{} at {} — {} models: {:?}", p.name, p.url, p.model_count, p.models);
}

// Chat with whatever is available
if let Some(p) = providers.first() {
    let reply = p.provider.chat(&p.models[0], "Hello!").await?;
    println!("{reply}");
}
# Ok(())
# }
```

Checks `OLLAMA_HOST` / `LM_STUDIO_URL` env vars, falls back to default ports.
Pass extra URLs for custom endpoints: `detect(&["http://gpu-server:11434"])`.

## Serve Your Model

Expose your local LLM as an OpenAI-compatible API that anyone can connect to:

```toml
[dependencies]
ai_assistant_core = { version = "0.2", features = ["serve"] }
```

```rust
use ai_assistant_core::{ollama, serve};

#[tokio::main]
async fn main() -> Result<(), ai_assistant_core::Error> {
    let provider = ollama();
    // Starts an OpenAI-compatible server on :8090
    serve::quick(provider).await?;
    Ok(())
}
```

With builder for more control:

```rust
use ai_assistant_core::{ollama, ProviderServiceBuilder};

# async fn example() -> Result<(), ai_assistant_core::Error> {
let provider = ollama();
ProviderServiceBuilder::new(provider)
    .port(9090)
    .token("my_secret")       // require auth
    .nat()                    // enable NAT traversal
    .start()
    .await?;
# Ok(())
# }
```

Endpoints provided:
- `GET /health` — health check
- `GET /v1/models` — list available models
- `POST /v1/chat/completions` — chat (streaming supported)

## Remote Access (NAT Traversal)

Enable the `nat` feature to discover your public IP via STUN and automatically open ports via UPnP or NAT-PMP:

```toml
[dependencies]
ai_assistant_core = { version = "0.2", features = ["serve", "nat"] }
```

When you call `.nat()` on the builder, it will:
1. Query STUN servers to discover your public IP
2. Attempt UPnP IGD port mapping on your router
3. Fall back to NAT-PMP if UPnP fails
4. Report the public URL in `ServiceInfo`

No need for ngrok, cloudflare tunnel, or VPN.

## Standalone Binary

Compile and run `ai_serve` for a zero-code way to share your model:

```bash
cargo install ai_assistant_core --bin ai_serve --features serve

# Auto-detect backend and serve
ai_serve

# With NAT traversal and authentication
ai_serve --nat --token my_secret

# Explicit backend
ai_serve --backend http://192.168.1.10:11434 --provider ollama --port 9090
```

Any OpenAI-compatible client can then connect:

```bash
curl http://your-ip:8090/v1/models
curl -X POST http://your-ip:8090/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer my_secret" \
  -d '{"model":"llama3.2:1b","messages":[{"role":"user","content":"Hello!"}]}'
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
