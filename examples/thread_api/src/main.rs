//! Lists the threads of one scope and prints the history of one thread,
//! through the threads service of a LiveKit Cloud project.
//!
//! This is the read side of an Agent 2.0 thread: the listing and the
//! history. A message to the agent goes to the agent endpoint instead, which
//! this example does not do.

use std::env;
use std::error::Error;

use chrono::DateTime;
use livekit_api::access_token::get_env_keys;
use livekit_api::services::threads::ThreadClient;
use livekit_protocol::agent::agent_thread::{
    GetItemsRequest, Item, ListRequest, SortOrder, Thread,
};
use livekit_protocol::agent::chat_context::chat_item;
use livekit_protocol::agent::chat_message::chat_content::Payload;
use livekit_protocol::agent::{ChatMessage, ChatRole};

const PAGE_SIZE: i32 = 50;
const DEFAULT_SCOPE: &str = "demo-user";

/// A protobuf timestamp as `YYYY-MM-DD HH:MM:SS` in UTC.
fn time(seconds: i64, nanos: i32) -> String {
    DateTime::from_timestamp(seconds, nanos.max(0) as u32)
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "-".to_owned())
}

/// The text parts of a message, joined.
fn text_of(message: &ChatMessage) -> String {
    message
        .content
        .iter()
        .filter_map(|content| content.payload.as_ref().map(|Payload::Text(text)| text.as_str()))
        .collect()
}

/// One history item as a line: who said or did what.
fn describe(item: &chat_item::Item) -> String {
    match item {
        chat_item::Item::Message(message) => {
            let role = ChatRole::try_from(message.role)
                .map(|role| role.as_str_name().to_lowercase())
                .unwrap_or_else(|_| "unknown".to_owned());
            let interrupted = if message.interrupted { " [interrupted]" } else { "" };
            format!("{role}: {}{interrupted}", text_of(message))
        }
        chat_item::Item::FunctionCall(call) => {
            format!("tool call {}({})", call.name, call.arguments)
        }
        chat_item::Item::FunctionCallOutput(output) => {
            let error = if output.is_error { " [error]" } else { "" };
            format!("tool output {}: {}{error}", output.name, output.output)
        }
        chat_item::Item::AgentHandoff(handoff) => format!("handoff to {}", handoff.new_agent_id),
        chat_item::Item::AgentConfigUpdate(_) => "agent config update".to_owned(),
    }
}

/// When a history item was created, in the same format as [`time`].
fn created_at(item: &chat_item::Item) -> String {
    let stamp = match item {
        chat_item::Item::Message(message) => message.created_at.as_ref(),
        chat_item::Item::FunctionCall(call) => call.created_at.as_ref(),
        chat_item::Item::FunctionCallOutput(output) => output.created_at.as_ref(),
        chat_item::Item::AgentHandoff(handoff) => handoff.created_at.as_ref(),
        chat_item::Item::AgentConfigUpdate(_) => None,
    };
    stamp.map(|t| time(t.seconds, t.nanos)).unwrap_or_else(|| "-".to_owned())
}

/// Every thread of `scope`, newest first, across all pages.
async fn list_threads(client: &ThreadClient, scope: &str) -> Result<Vec<Thread>, Box<dyn Error>> {
    let mut threads = Vec::new();
    let mut page_token = String::new();
    loop {
        let request = ListRequest { scope: scope.to_owned(), page_token, limit: PAGE_SIZE };
        let page = client.list(request).await?;
        threads.extend(page.threads);
        if page.next_page_token.is_empty() {
            return Ok(threads);
        }
        page_token = page.next_page_token;
    }
}

/// Every history item of `thread_id`, oldest first, from every agent's lane.
async fn list_items(client: &ThreadClient, thread_id: &str) -> Result<Vec<Item>, Box<dyn Error>> {
    let mut items = Vec::new();
    let mut page_token = String::new();
    loop {
        let request = GetItemsRequest {
            thread_id: thread_id.to_owned(),
            agent: String::new(),
            page_token,
            limit: PAGE_SIZE,
            sort_order: SortOrder::Asc as i32,
        };
        let page = client.items(request).await?;
        items.extend(page.items);
        if page.next_page_token.is_empty() {
            return Ok(items);
        }
        page_token = page.next_page_token;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let url = env::var("LIVEKIT_URL")
        .map_err(|_| "set LIVEKIT_URL, e.g. wss://<project>.staging.livekit.cloud")?;
    let (api_key, api_secret) =
        get_env_keys().map_err(|_| "set LIVEKIT_API_KEY and LIVEKIT_API_SECRET")?;
    let client = ThreadClient::with_api_key(&url, &api_key, &api_secret);

    let scope = env::var("THREAD_SCOPE").unwrap_or_else(|_| DEFAULT_SCOPE.to_owned());
    let threads = list_threads(&client, &scope).await?;
    println!("Threads of scope {scope:?}, newest first:");
    for thread in &threads {
        let last_active =
            thread.last_active.as_ref().map(|t| time(t.seconds, t.nanos)).unwrap_or_default();
        let title = thread.attributes.get("title").map(String::as_str).unwrap_or("(no title)");
        println!("  {:<28} {:<20} {title}", thread.thread_id, last_active);
    }

    let thread_id = match env::var("THREAD_ID") {
        Ok(id) => id,
        Err(_) => match threads.first() {
            Some(thread) => thread.thread_id.clone(),
            None => {
                println!("No threads in scope {scope:?}.");
                return Ok(());
            }
        },
    };

    let items = list_items(&client, &thread_id).await?;
    println!();
    println!("History of {thread_id}, {} items:", items.len());
    for item in &items {
        let Some(chat_item) = item.item.as_ref().and_then(|wrapper| wrapper.item.as_ref()) else {
            continue;
        };
        println!("  [{}] {} {}", item.agent, created_at(chat_item), describe(chat_item));
    }
    Ok(())
}
