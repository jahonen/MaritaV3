//! AI station-agent harness for MaritaV3.
//!
//! Connects to a running `marita serve` instance, subscribes to the tick
//! stream, and drives one or more stations via the gRPC `StreamStationCommands`
//! RPC.  The LLM is a replaceable adapter; the MVP default is Hermes 3 8B via
//! Ollama, with a deterministic fallback when no LLM endpoint is configured.
//!
//! The agent architecture is intentionally split:
//!   - the **LLM** proposes high-level actions in response to station state;
//!   - **deterministic tools** validate constraints and translate proposals into
//!     `StationCommand` messages;
//!   - the **Marita engine** performs all bookkeeping (warehouses, production,
//!     signal propagation, market-post history).

mod agent;
mod llm;
mod tools;

use agent::StationAgent;
use llm::{DeterministicLlm, HermesOllamaLlm, LlmAdapter};
use std::sync::Arc;

#[derive(Debug, Clone)]
struct Args {
    addr: String,
    station_id: u64,
    ollama_url: Option<String>,
    ollama_model: String,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            addr: "http://127.0.0.1:50051".into(),
            station_id: 2000,
            ollama_url: Some("http://localhost:11434".into()),
            ollama_model: "hermes3:8b".into(),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args();

    let llm: Arc<dyn LlmAdapter + Send + Sync> = match args.ollama_url {
        Some(base_url) => Arc::new(HermesOllamaLlm::new(base_url, args.ollama_model)),
        None => Arc::new(DeterministicLlm::new()),
    };

    let agent = StationAgent::new(args.addr, args.station_id, llm);
    agent.run().await?;
    Ok(())
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--addr" => {
                if let Some(v) = iter.next() {
                    args.addr = v;
                }
            }
            "--station-id" => {
                if let Some(v) = iter.next() {
                    args.station_id = v.parse().unwrap_or(args.station_id);
                }
            }
            "--ollama-url" => {
                if let Some(v) = iter.next() {
                    args.ollama_url = Some(v);
                }
            }
            "--ollama-model" => {
                if let Some(v) = iter.next() {
                    args.ollama_model = v;
                }
            }
            "--no-llm" => args.ollama_url = None,
            _ => {}
        }
    }
    args
}
