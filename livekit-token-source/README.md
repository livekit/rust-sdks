# LiveKit Token Source

Token sources for the LiveKit Rust SDK. A token source procures the credentials — server URL and
participant token — needed to join a LiveKit room.

Three sources ship with the crate, constructed via factory functions:

- `literal` — a fixed set of pre-provisioned credentials.
- `endpoint` — fetches credentials from a token endpoint implementing the
  [standard format](https://docs.livekit.io/frontends/build/authentication/endpoint/).
- `development_token_server` — queries a LiveKit
  [development token server](https://docs.livekit.io/frontends/build/authentication/sandbox-token-server/)
  for prototyping. **Not for production use.**

Custom credential backends can implement the `TokenSourceFixed` or `TokenSourceConfigurable`
traits.

```rust
use livekit_token_source::{TokenSourceConfigurable, TokenSourceFetchOptions};

let source = livekit_token_source::endpoint("https://example.com/api/token");
let options = TokenSourceFetchOptions::new()
    .with_room_name("my-room")
    .with_participant_identity("user-123");
let response = source.fetch(&options).await?;
// connect with response.server_url / response.participant_token
```

See [examples/token_source](../examples/token_source) for a runnable example.
