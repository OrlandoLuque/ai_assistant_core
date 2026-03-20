//! `ai_serve` — Expose your local LLM as an OpenAI-compatible API.
//!
//! Usage:
//!   ai_serve                                    # auto-detect backend
//!   ai_serve --port 9090                        # custom port
//!   ai_serve --backend http://localhost:11434    # explicit backend
//!   ai_serve --nat --token my_secret            # NAT traversal + auth
//!   ai_serve --help                             # show help

use ai_assistant_core::{detect, ollama, ollama_at, lm_studio, openai_compat};
use ai_assistant_core::serve::ProviderServiceBuilder;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return;
    }

    let port = get_arg(&args, "--port").and_then(|v| v.parse().ok()).unwrap_or(8090u16);
    let backend_url = get_arg(&args, "--backend");
    let provider_type = get_arg(&args, "--provider");
    let token = get_arg(&args, "--token");
    let enable_nat = args.iter().any(|a| a == "--nat");
    let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");

    if verbose {
        eprintln!("ai_serve v{}", env!("CARGO_PKG_VERSION"));
    }

    // Determine provider
    let provider = if let Some(url) = backend_url {
        let ptype = provider_type.as_deref().unwrap_or("ollama");
        match ptype {
            "ollama" => ollama_at(&url),
            "lmstudio" | "lm-studio" => openai_compat(&format!("{}/v1", url.trim_end_matches("/v1"))),
            _ => openai_compat(&url),
        }
    } else if let Some(ptype) = provider_type {
        match ptype.as_str() {
            "ollama" => ollama(),
            "lmstudio" | "lm-studio" => lm_studio(),
            _ => {
                eprintln!("Unknown provider: {}. Use: ollama, lmstudio, openai-compat", ptype);
                std::process::exit(1);
            }
        }
    } else {
        // Auto-detect
        if verbose {
            eprintln!("Auto-detecting backend...");
        }
        let detected = detect(&[]).await;
        if let Some(first) = detected.into_iter().next() {
            if verbose {
                eprintln!("Found {} at {} ({} models)", first.name, first.url, first.model_count);
            }
            first.provider
        } else {
            eprintln!("No LLM provider detected. Make sure Ollama or LM Studio is running.");
            eprintln!("Or specify manually: ai_serve --backend http://localhost:11434 --provider ollama");
            std::process::exit(1);
        }
    };

    // Build service
    let mut builder = ProviderServiceBuilder::new(provider).port(port);

    if let Some(t) = token {
        builder = builder.token(&t);
    }

    if enable_nat {
        builder = builder.nat();
    }

    // Start
    match builder.start().await {
        Ok(_info) => {}
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn print_help() {
    println!("ai_serve — Expose your local LLM as an OpenAI-compatible API");
    println!();
    println!("USAGE:");
    println!("    ai_serve [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    --port <PORT>         HTTP port (default: 8090)");
    println!("    --backend <URL>       Backend URL (default: auto-detect)");
    println!("    --provider <TYPE>     ollama | lmstudio | openai-compat");
    println!("    --token <TOKEN>       Require bearer token for access");
    println!("    --nat                 Enable NAT traversal (STUN + UPnP)");
    println!("    --verbose, -v         Verbose output");
    println!("    --help, -h            Show this help");
    println!();
    println!("EXAMPLES:");
    println!("    ai_serve                                     # auto-detect, serve on :8090");
    println!("    ai_serve --nat --token secret                # with NAT + auth");
    println!("    ai_serve --backend http://192.168.1.10:11434 # explicit Ollama");
    println!();
    println!("ENDPOINTS:");
    println!("    GET  /health                   Health check");
    println!("    GET  /v1/models                List models");
    println!("    POST /v1/chat/completions      Chat (streaming supported)");
}
