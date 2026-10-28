# LiveKit Session

Agent sessions for the LiveKit Rust SDK. A `Session` is the single entry point for a conversation
with an agent: it fetches credentials from a [token source](../livekit-token-source), connects a
room, projects what the agent is doing, and merges chat and transcripts into one ordered message
feed. It follows the same model as the Session in the JavaScript, Swift, Android and Flutter SDKs.

```rust
use livekit_session::{Session, SessionOptions};
use livekit_token_source::{development_token_server, TokenSourceFetchOptions};

let source = development_token_server("<token server id>");
let options = TokenSourceFetchOptions::new().with_agent_name("my-agent");
let session = Session::new((source, options), SessionOptions::default());

session.start().await?;
session.state().wait_for(|state| state.agent_state.is_connected()).await?;
session.send_text("Hello!").await?;
```

See [`examples/session`](../examples/session) for a complete text chat.

## Credentials

`Session::new` takes anything convertible into a `SessionTokenSource`:

- a configurable source paired with its fetch options, as above. The options are where the agent
  to dispatch is named;
- a `literal` source holding a pre-generated token;
- any other fixed source, wrapped with `SessionTokenSource::fixed`.

When the room disconnects without the app asking for it, the session calls
`TokenSourceConfigurable::invalidate` before reporting the disconnect. Agents do not rejoin a room
that already existed, so the next `start` needs fresh credentials rather than cached ones.

## Lifecycle

`start` fetches credentials and connects the room. It resolves once the room is connected, not
once the agent is. Calling it again while the session is connecting or connected fails with
`AlreadyStarted`. `end` closes the room, or cancels a `start` still in progress.

Dropping the last `Session` handle also closes the room. `room()` returns the connected room for
anything the session does not do itself, such as publishing a microphone.

## State

`state()` returns a `tokio::sync::watch::Receiver<SessionSnapshot>`, and `snapshot()` returns its
current value. A snapshot holds the connection state and the agent state:

| Agent state | Meaning |
| --- | --- |
| `Disconnected` | The session is not connected |
| `Connecting` | No agent is in the room yet |
| `Initializing`, `Idle` | The agent joined but is not in a conversation |
| `Listening`, `Thinking`, `Speaking` | The agent is in a conversation |
| `Failed(Timeout)` | The token dispatched an agent, but none became conversational within `agent_connect_timeout` (20 s by default) |
| `Failed(Left)` | The agent left while the room stayed connected |
| `Unknown` | The agent reports an activity this SDK does not recognize |

`Failed` is not final: an agent that joins, or becomes conversational, replaces it. Most code
needs only the predicates on `AgentState`: `is_connected`, `can_listen`, `is_pending` and
`is_finished`.

The agent's microphone and its avatar's camera are available from `agent_audio_track()` and
`agent_avatar_video_track()`.

## Messages

`messages()` returns the feed, `subscribe()` streams changes to it, and `send_text` sends chat to
the agent. The feed merges:

- transcripts of the agent and of the local user, from `lk.transcription`, updated in place as
  they grow;
- the local user's input sent with `send_text`, added once the send succeeds;
- chat sent by other participants, from `lk.chat`.

Messages are ordered by `seq`, the order in which they were first received, rather than by their
timestamps, which come from the sender's clock. `restore_messages` replaces the feed with a saved
conversation.

## Things the session owns

- **The `lk.chat` and `lk.transcription` text streams.** The session takes their readers, so an
  app subscribed to the room sees those `TextStreamOpened` events with the reader already taken.
  Streams on other topics are left to the app.
- **No media.** The session never publishes a microphone. Publish one through `room()` to talk to
  the agent by voice; `can_listen` is only meaningful once you have.
