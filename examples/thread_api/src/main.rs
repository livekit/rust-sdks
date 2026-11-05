//! Agent 2.0 threads of a LiveKit Cloud project, in two modes:
//!
//! - `cargo run -p thread_api -- list_threads` lists the threads of one scope
//!   and prints the history of one thread, through the threads service.
//! - `cargo run -p thread_api -- chat` chats with the agent in one thread,
//!   through `livekit-thread`. `/call` starts or ends a voice call in the same
//!   thread.
//!
//! Environment: `LIVEKIT_URL`, `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`, and
//! optionally `THREAD_SCOPE` (default `demo-user`), `THREAD_ID` (default: the
//! newest thread for `list_threads`, a new thread for `chat`), `AGENT_NAME`
//! (default `dentist`), `AGENT_ENDPOINT` (default `frontend`) and
//! `LIVEKIT_AGENT_DEPLOYMENT` (the agent worker's deployment).

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::io::Write;

use chrono::DateTime;
use livekit_api::access_token::get_env_keys;
use livekit_api::services::threads::ThreadClient;
use livekit_protocol::agent::agent_thread::{
    GetItemsRequest, Item, ListRequest, SortOrder, Thread,
};
use livekit_protocol::agent::chat_context::chat_item;
use livekit_protocol::agent::chat_message::chat_content::Payload;
use livekit_protocol::agent::{ChatMessage, ChatRole};
use livekit_thread::{
    Message, MessageOrigin, SessionOptions, SessionState, ThreadDirectory, ThreadDirectoryOptions,
    ThreadEvent, TurnOutcome,
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{mpsc, watch};

const PAGE_SIZE: i32 = 50;
const DEFAULT_SCOPE: &str = "demo-user";
const DEFAULT_AGENT: &str = "dentist";
const DEFAULT_ENDPOINT: &str = "frontend";

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

/// Lists the threads of the scope and prints the history of one thread,
/// directly through the threads service.
async fn list_threads_command(
    url: &str,
    api_key: &str,
    api_secret: &str,
    scope: &str,
) -> Result<(), Box<dyn Error>> {
    let client = ThreadClient::with_api_key(url, api_key, api_secret);
    let threads = list_threads(&client, scope).await?;
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

/// An interactive chat with the agent in one thread, through
/// `livekit-thread`. Opens `THREAD_ID`, or creates a new thread.
async fn chat(
    url: &str,
    api_key: &str,
    api_secret: &str,
    scope: &str,
) -> Result<(), Box<dyn Error>> {
    // For development only, the directory signs every token locally with the
    // project key.
    let directory = ThreadDirectory::with_api_key(
        url,
        api_key,
        api_secret,
        ThreadDirectoryOptions {
            agent_name: env::var("AGENT_NAME").unwrap_or_else(|_| DEFAULT_AGENT.to_owned()),
            endpoint: env::var("AGENT_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.to_owned()),
            scope: scope.to_owned(),
            // The same variable the agent worker registers its deployment with.
            deployment: env::var("LIVEKIT_AGENT_DEPLOYMENT").ok(),
        },
    );
    let thread = match env::var("THREAD_ID") {
        Ok(id) => directory.open(&id).await?,
        Err(_) => directory.create().await?,
    };

    // Subscribe before reading the history, so that no update is lost.
    let events = thread.subscribe();
    println!(
        "Thread {}. Type a message, /call to start or end a call, /quit to leave.",
        thread.id()
    );
    for message in thread.messages() {
        print_message(&message);
    }
    let feed = tokio::spawn(print_feed(events));
    prompt();

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut in_call = false;
    let mut call_state = None;
    while let Some(line) = lines.next_line().await? {
        match line.trim() {
            "" => prompt(),
            "/quit" => break,
            "/call" => {
                let call = match thread.call() {
                    Some(call) => call,
                    None => {
                        let call = thread.create_call(SessionOptions::default())?;
                        call_state = Some(tokio::spawn(print_call_state(call.state())));
                        call
                    }
                };
                if in_call {
                    call.end().await;
                    in_call = false;
                } else {
                    match call.start().await {
                        Ok(()) => {
                            in_call = true;
                            println!("(in a call: speak now, /call to end it)");
                        }
                        Err(error) => println!("(call failed: {error})"),
                    }
                }
                prompt();
            }
            text => {
                // The feed prints the answer and the next prompt.
                if let Err(error) = thread.send_text(text).await {
                    println!("(not sent: {error})");
                    prompt();
                }
            }
        }
    }

    // Ending the thread also ends the call, and closes the feed.
    thread.end().await;
    feed.await?;
    if let Some(call_state) = call_state {
        call_state.abort();
    }
    Ok(())
}

/// Prints the call's connection and agent state when they change.
async fn print_call_state(mut state: watch::Receiver<SessionState>) {
    while state.changed().await.is_ok() {
        let current = *state.borrow_and_update();
        println!("(call: {:?}, agent: {:?})", current.connection, current.agent);
    }
}

/// Prints the feed: the agent's text answer as it streams in, other messages
/// when they are complete, and a prompt after each turn.
async fn print_feed(mut events: mpsc::UnboundedReceiver<ThreadEvent>) {
    // How much of each streaming answer is already printed, by message id.
    let mut printed: HashMap<String, usize> = HashMap::new();
    while let Some(event) = events.recv().await {
        match event {
            ThreadEvent::MessageUpserted(message) if message.origin == MessageOrigin::AgentText => {
                let done = printed.get(&message.id).copied();
                match done {
                    None => print!("agent: {}", message.text),
                    Some(len) if message.text.get(..len).is_some() => {
                        print!("{}", &message.text[len..])
                    }
                    // The answer was replaced, not extended: print it again.
                    Some(_) => print!("\nagent: {}", message.text),
                }
                if message.is_final {
                    println!();
                    printed.remove(&message.id);
                } else {
                    printed.insert(message.id.clone(), message.text.len());
                }
                flush();
            }
            // The user's own text is already on screen.
            ThreadEvent::MessageUpserted(message)
                if message.is_final && message.origin != MessageOrigin::UserInput =>
            {
                print_message(&message);
            }
            ThreadEvent::TurnEnded(TurnOutcome::Completed | TurnOutcome::InputRequired) => prompt(),
            ThreadEvent::TurnEnded(outcome) => {
                println!("(turn ended: {outcome:?})");
                prompt();
            }
            _ => {}
        }
    }
}

fn print_message(message: &Message) {
    let speaker = match message.origin {
        MessageOrigin::UserInput => "you",
        MessageOrigin::UserTranscript => "you (spoken)",
        MessageOrigin::AgentText => "agent",
        MessageOrigin::AgentTranscript => "agent (spoken)",
        _ => message.participant_identity.as_str(),
    };
    println!("{speaker}: {}", message.text);
}

fn prompt() {
    print!("> ");
    flush();
}

fn flush() {
    // A failed flush of the terminal is not worth stopping the chat for.
    let _ = std::io::stdout().flush();
}

const USAGE: &str = "usage: cargo run -p thread_api -- <list_threads | chat>";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let command = env::args().nth(1).ok_or(USAGE)?;
    let url = env::var("LIVEKIT_URL")
        .map_err(|_| "set LIVEKIT_URL, e.g. wss://<project>.staging.livekit.cloud")?;
    let (api_key, api_secret) =
        get_env_keys().map_err(|_| "set LIVEKIT_API_KEY and LIVEKIT_API_SECRET")?;
    let scope = env::var("THREAD_SCOPE").unwrap_or_else(|_| DEFAULT_SCOPE.to_owned());

    match command.as_str() {
        "list_threads" => list_threads_command(&url, &api_key, &api_secret, &scope).await,
        "chat" => chat(&url, &api_key, &api_secret, &scope).await,
        _ => Err(USAGE.into()),
    }
}
