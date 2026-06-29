// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Lightweight A2A-compliant Agent in Rust.
//!
//! Implements:
//! - `GET  /.well-known/agent.json` / `agent-card.json` — agent card (capability discovery)
//! - `POST /message:stream`                            — streaming message (SSE response)
//!
//! Run with:
//!   cargo run -p a2a_mock_agent -- --port 8000

use std::convert::Infallible;
use std::net::SocketAddr;

use axum::{
    extract::Json,
    response::sse::{Event, Sse},
    routing::{get, post},
    Router,
};
use log::info;
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendMessageRequest {
    message: Option<serde_json::Value>,
    #[allow(dead_code)]
    configuration: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Core Business Logic
// ---------------------------------------------------------------------------

/// Parses a simple currency conversion request (e.g. "convert 100 USD to EUR")
fn parse_conversion(text: &str) -> Option<(f64, String, String)> {
    let lower = text.to_lowercase();

    // 1. Find the first number in the string
    let mut amount = None;
    for word in lower.split_whitespace() {
        // Strip trailing punctuation like dots or question marks
        let clean_num = word.trim_matches(|c: char| !c.is_numeric() && c != '.');
        if let Ok(val) = clean_num.parse::<f64>() {
            amount = Some(val);
            break;
        }
    }

    let amount = amount?;

    // 2. Identify currencies
    let currencies = ["usd", "eur", "gbp", "inr", "jpy", "dollar", "euro", "pound", "rupee", "yen"];
    let mut found = Vec::new();
    for word in lower.split_whitespace() {
        let clean_word = word.trim_matches(|c: char| !c.is_alphabetic());
        for &curr in &currencies {
            if clean_word.starts_with(curr) {
                let code = match curr {
                    "usd" | "dollar" => "USD",
                    "eur" | "euro" => "EUR",
                    "gbp" | "pound" => "GBP",
                    "inr" | "rupee" => "INR",
                    "jpy" | "yen" => "JPY",
                    _ => continue,
                };
                if found.last() != Some(&code.to_string()) {
                    found.push(code.to_string());
                }
            }
        }
    }

    if found.len() >= 2 {
        Some((amount, found[0].clone(), found[1].clone()))
    } else {
        None
    }
}

/// Converts currency using fixed exchange rates
fn perform_conversion(amount: f64, from: &str, to: &str) -> String {
    let rates = [("USD", 1.0), ("EUR", 0.92), ("GBP", 0.78), ("INR", 83.50), ("JPY", 155.20)];
    let get_rate =
        |code: &str| -> Option<f64> { rates.iter().find(|&&(c, _)| c == code).map(|&(_, r)| r) };

    if let (Some(r_from), Some(r_to)) = (get_rate(from), get_rate(to)) {
        let converted = amount * (r_to / r_from);
        format!(
            "{:.2} {} is equal to {:.2} {} (exchange rate: {:.4}).",
            amount,
            from,
            converted,
            to,
            r_to / r_from
        )
    } else {
        format!("Sorry, I cannot convert between {} and {}.", from, to)
    }
}

/// Query local Ollama if available
async fn query_ollama(prompt: &str) -> Result<String, reqwest::Error> {
    let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen3.5:0.8b".to_string());
    let host =
        std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
    let endpoint = format!("{}/api/chat", host.trim_end_matches('/'));

    info!("Ollama: Sending prompt to model {model} via {endpoint}...");
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build()?;

    let start = std::time::Instant::now();
    let res = client
        .post(&endpoint)
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are a helpful currency conversion agent. The current exchange rates are: USD=1.0, EUR=0.92, GBP=0.78, INR=83.50, JPY=155.20. When the user asks to convert, calculate the result using these rates and reply with a friendly, natural sentence."
                },
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "stream": false
        }))
        .send()
        .await?;

    let body = res.json::<serde_json::Value>().await?;
    let content = body
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("No response from assistant.")
        .to_string();

    info!("Ollama: Received response in {}ms", start.elapsed().as_millis());
    Ok(content)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Serve the agent card so `A2AClient::from_card_url` can discover us.
async fn agent_card() -> axum::response::Json<serde_json::Value> {
    axum::response::Json(json!({
        "name": "RustCurrencyAgent",
        "description": "Lightweight, low-power A2A Currency Conversion Agent in Rust",
        "version": "1.0.0",
        "supportedInterfaces": [
            {
                "url": "http://127.0.0.1:8000",
                "protocolBinding": "HTTP+JSON",
                "protocolVersion": "1.0"
            }
        ],
        "capabilities": {
            "streaming": true
        },
        "defaultInputModes": ["text/plain"],
        "defaultOutputModes": ["text/plain"],
        "skills": [
            {
                "id": "currency-conversion",
                "name": "Currency Conversion",
                "description": "Convert amounts between USD, EUR, GBP, INR, and JPY",
                "inputModes": ["text/plain"],
                "outputModes": ["text/plain"]
            }
        ]
    }))
}

/// Handle `POST /message:stream`.
async fn stream_message(
    Json(req): Json<SendMessageRequest>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let msg_id = req
        .message
        .as_ref()
        .and_then(|m| m.get("messageId"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let task_id = Uuid::new_v4().to_string();
    let context_id = Uuid::new_v4().to_string();

    let user_text = req
        .message
        .as_ref()
        .and_then(|m| m.get("parts"))
        .and_then(|p| p.as_array())
        .and_then(|arr| arr.first())
        .and_then(|part| part.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    info!("stream_message: request msg={}, text=\"{}\"", msg_id, user_text);

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(10);

    tokio::spawn(async move {
        // 1. Emit TASK_STATE_WORKING
        let working_evt = Event::default().json_data(&json!({
            "statusUpdate": {
                "taskId": &task_id,
                "contextId": &context_id,
                "status": {
                    "state": "TASK_STATE_WORKING",
                    "message": {
                        "parts": [{"text": "Calculating exchange rate...", "mediaType": "text/plain"}]
                    },
                    "timestamp": "2026-06-11T00:00:00Z"
                }
            }
        })).unwrap();
        let _ = tx.send(Ok(working_evt)).await;

        // 2. Compute response (Local regex -> fallback to Ollama -> fallback to offline info)
        let reply_text = if user_text.trim().is_empty() {
            info!("User text is empty; returning default greeting");
            "Hello! I am a currency conversion agent. Try saying: 'Convert 100 USD to EUR'."
                .to_string()
        } else if let Some((amount, from, to)) = parse_conversion(&user_text) {
            info!("Local Rust parser matched: {} {} to {}", amount, from, to);
            perform_conversion(amount, &from, &to)
        } else {
            info!("Local parser did not match. Attempting Ollama query...");
            match query_ollama(&user_text).await {
                Ok(reply) => reply,
                Err(e) => {
                    log::warn!("Ollama connection failed: {}. Offline fallback.", e);
                    "I am running in lightweight local mode. I can convert currencies. Try: 'Convert 100 USD to EUR'."
                        .to_string()
                }
            }
        };

        info!("Sending agent response: \"{}\"", reply_text);

        // Stream word-by-word with a small delay for natural pacing
        let words: Vec<&str> = reply_text.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            let chunk = if i == 0 { word.to_string() } else { format!(" {word}") };
            let chunk_evt = Event::default()
                .json_data(&json!({
                    "message": {
                        "messageId": Uuid::new_v4().to_string(),
                        "contextId": &context_id,
                        "taskId": &task_id,
                        "role": "ROLE_AGENT",
                        "parts": [
                            {
                                "mediaType": "text/plain",
                                "text": chunk,
                            }
                        ]
                    }
                }))
                .unwrap();

            let _ = tx.send(Ok(chunk_evt)).await;
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }

        // 3. Emit TASK_STATE_COMPLETED
        let completed_evt = Event::default()
            .json_data(&json!({
                "statusUpdate": {
                    "taskId": &task_id,
                    "contextId": &context_id,
                    "status": {
                        "state": "TASK_STATE_COMPLETED",
                        "message": {
                            "parts": [{"text": "Done.", "mediaType": "text/plain"}]
                        },
                        "timestamp": "2026-06-11T00:00:01Z"
                    }
                }
            }))
            .unwrap();
        let _ = tx.send(Ok(completed_evt)).await;
    });

    Sse::new(ReceiverStream::new(rx))
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    // Find port from command line arguments, default to 8000 to match agent-url config
    let mut port = 8000;
    let args: Vec<String> = std::env::args().collect();
    for i in 0..args.len() {
        if (args[i] == "--port" || args[i] == "-p") && i + 1 < args.len() {
            if let Ok(p) = args[i + 1].parse() {
                port = p;
            }
        }
    }

    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let app = Router::new()
        .route("/.well-known/agent.json", get(agent_card))
        .route("/.well-known/agent-card.json", get(agent_card))
        .route("/message:stream", post(stream_message));

    info!("Rust A2A agent listening on http://{addr}");
    info!("  Agent card: http://{addr}/.well-known/agent-card.json");
    info!("  Streaming:  http://{addr}/message:stream");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
