# LiveKit Session

Agent sessions for the LiveKit Rust SDK. A `Session` is the single entry point for a conversation
with an agent: it fetches credentials from a [token source](../livekit-token-source), connects a
room, publishes the microphone, projects what the agent is doing, and merges chat and transcripts
into one ordered message feed. It follows the same model as the Session in the JavaScript, Swift,
Android and Flutter SDKs.

```rust
use livekit_session::{Session, SessionOptions};
use livekit_token_source::{development_token_server, TokenSourceFetchOptions};

let token_source = development_token_server("<token server id>");
let fetch_options = TokenSourceFetchOptions::new().with_agent_name("my-agent");
let session = Session::new(SessionOptions::new_configurable_source(token_source, fetch_options));

session.start().await?;
session.state().wait_for(|state| state.agent.is_connected()).await?;
session.send_text("Hello!").await?;
```

See [`examples/session`](../examples/session) for a complete text chat.

## Credentials

`Session::new` takes a `SessionOptions`, which holds the token source. Which `SessionOptions`
constructor to use depends on the kind of token source:

- `SessionOptions::new_configurable_source` takes a configurable source with its
  `TokenSourceFetchOptions`, for the agent to dispatch (as above), agent metadata, a room name or a
  participant identity;
- `SessionOptions::new_fixed_source` takes a fixed source, such as a `literal` one holding a
  pre-generated token.

Every `start` fetches credentials anew, so pass the source itself rather than one wrapped in
`cached()`. Agents do not rejoin a room that already existed, and a cached token names the previous
room.

## Lifecycle

`start` fetches credentials and connects the room. It resolves once the room is connected, not
once the agent is. Calling it again while the session is connecting or connected fails with
`AlreadyStarted`. `end` closes the room, or cancels a `start` still in progress, and does nothing
when there is nothing to end.

Dropping the last `Session` handle also closes the room. `room()` returns the connected room for
anything the session does not do itself, such as publishing a camera.

## State

`state()` returns a `tokio::sync::watch::Receiver<SessionState>`: `borrow()` reads the current
value, and `wait_for` awaits a condition. A `SessionState` holds the connection state (`connection`)
and the agent state (`agent`):

| Agent state | Meaning |
| --- | --- |
| `Disconnected` | The session is not connected |
| `Connecting` | No agent is in the room yet |
| `Joined(agent)` | An agent joined the room, in a conversation or not |
| `Failed(Timeout { .. })` | No agent became conversational within `agent_connect_timeout` (20 s by default) of connecting |
| `Failed(Left)` | The agent left while the room stayed connected |

`Joined` carries an `Agent`: its identity, its tracks, and its `AgentActivity`, what the agent
reports through its `lk.agent.state` attribute. `Failed` is not final: an agent that joins, or
becomes conversational, replaces it. An agent that timed out but is still in the room stays
available in `Timeout`.

Most code needs only the methods on `AgentState`:

- `is_connected`, `is_pending` and `is_finished` tell where the agent is. `is_connected` is true
  only while the agent is listening, thinking or speaking;
- `activity()` and `identity()` tell what the agent in the room is doing and who it is;
- `audio_track()` and `avatar_video_track()` return the agent's microphone and its avatar's camera,
  for an app that wants their frames, e.g. for a visualizer or to show the avatar.

```rust
let mut state = session.state();
let state = state.wait_for(|state| state.agent.audio_track().is_some()).await?.clone();
let audio = state.agent.audio_track();
```

The tracks are part of the state, so the state changes when a track arrives, goes, or is replaced.
A `SessionState` kept after the session moved on keeps its tracks alive: drop it when a newer one
arrives.

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
- **Platform audio.** By default `start` captures the microphone and publishes it as the local
  participant's microphone track, and the agent's audio plays through the speakers, until the
  session ends. `SessionOptions::with_platform_audio(false)` turns this off, for a conversation by text alone
  or to publish audio of your own through `room()`. Device selection and audio processing are
  shared by every `PlatformAudio` in the process, so configure them on one of your own.
