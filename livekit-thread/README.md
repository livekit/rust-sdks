# LiveKit Thread

Agent threads for the LiveKit Rust SDK. A `Thread` is one conversation with an Agent 2.0 agent
that lasts across text and calls: text goes to the agent's text endpoint over A2A, and the
thread's `Call` joins a room with the agent for the same thread. Both write into one feed of
`Message`s, the same type as in [`livekit-session`](../livekit-session), and one `ThreadState`
covers the text turns and the call.

A `ThreadDirectory` lists the threads of one scope (usually one user), creates new threads and
opens existing ones.

```rust
use livekit_thread::{SessionOptions, ThreadDirectory, ThreadDirectoryOptions};

let directory = ThreadDirectory::with_api_key(
    "wss://my-project.livekit.cloud",
    "api-key",
    "api-secret",
    ThreadDirectoryOptions {
        agent_name: "my-agent".into(),
        endpoint: "chat".into(),
        scope: "user-123".into(),
        deployment: None,
        call_options: SessionOptions::default(),
    },
);

let thread = directory.create().await?;
thread.send_text("Hello!").await?;
thread.state().wait_for(|state| state.turn.is_idle()).await?;

thread.call().start().await?;
thread.state().wait_for(|state| state.call.agent.is_connected()).await?;
```

`ThreadDirectory::with_api_key` signs every token locally with the project's API key and secret.
It is for development only, until a token service supplies the tokens. See
[`examples/thread_api`](../examples/thread_api) for the whole flow.
