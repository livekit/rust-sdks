# LiveKit Token Source

Token sources for the LiveKit Rust SDK. A token source procures the credentials — server URL and
participant token — needed to join a LiveKit room. Once a source is constructed, call `fetch` to
obtain a set of credentials.

## Fixed and configurable sources

Every token source is one of two kinds, each represented by a trait:

- **Fixed** (`TokenSourceFixed`) — `fetch()` takes no parameters; the credentials are decided
  ahead of time and every call resolves the same way.
- **Configurable** (`TokenSourceConfigurable`) — `fetch(&options)` takes
  `TokenSourceFetchOptions` that parameterize the credentials generated for that call: room name,
  participant identity and metadata, agent dispatch, and so on.

Combined with the mechanism used to procure credentials, this spans the following matrix:

| Mechanism    | Using pre-generated credentials | Via an HTTP request to a URL | Via fully custom logic |
| ------------ | ------------------------------- | ---------------------------- | ---------------------- |
| Fixed        | [`literal`](#literal) | — | implement `TokenSourceFixed` |
| Configurable | — | [`endpoint`](#endpoint) or [`development_token_server`](#development_token_server) | implement `TokenSourceConfigurable` |

## Sources shipped with the crate

Three sources ship with the crate, constructed via factory functions.

### `literal`

A fixed source holding a single set of pre-provisioned credentials, captured at construction and
returned as-is on every fetch — no I/O involved.

```rust
use livekit_token_source::TokenSourceFixed;

let source = livekit_token_source::literal("wss://example.livekit.cloud", "<participant token>");
let response = source.fetch().await?;
```

### `endpoint`

A configurable source that fetches credentials from a token endpoint implementing the
[standard format](https://docs.livekit.io/frontends/build/authentication/endpoint/). Each fetch
serializes the options into the standard JSON request body and `POST`s it with
`Content-Type: application/json`, plus any headers added via `with_header` / `with_headers`
(e.g. for authenticating against the endpoint). A 2xx response is parsed as the standard JSON
response format; any other status surfaces as `TokenSourceError::Server` carrying the status and
body.

Requests are sent through the process-wide HTTP client from `livekit-net`: on native builds the
built-in client is used automatically; embedders can register their own via
`livekit_net::set_http_client`.

```rust
use livekit_token_source::{TokenSourceConfigurable, TokenSourceFetchOptions};

let source = livekit_token_source::endpoint("https://example.com/api/token")
    .with_header("Authorization", "Bearer <endpoint credential>");
let options = TokenSourceFetchOptions::new()
    .with_room_name("my-room")
    .with_participant_identity("user-123");
let response = source.fetch(&options).await?;
// connect with response.server_url / response.participant_token
```

### `development_token_server`

A configurable source that queries a LiveKit
[development token server](https://docs.livekit.io/frontends/build/authentication/sandbox-token-server/)
for prototyping. Under the hood it is an `endpoint` source pre-configured with the LiveKit Cloud
development token server URL, authenticating via the `X-Sandbox-ID` header with the given token
server ID.

**This mechanism is inherently insecure and must not be used in production.**

## Custom credential backends

Custom credential backends implement the `TokenSourceFixed` or `TokenSourceConfigurable` trait
directly; pick the trait by whether the backend accepts per-fetch parameters. Code written
against the traits works the same with custom and shipped sources.

```rust
use async_trait::async_trait;
use livekit_token_source::{TokenSourceFixed, TokenSourceResponse, TokenSourceResult};

/// Reads credentials from a JSON file, e.g.
/// `{"server_url": "wss://...", "participant_token": "..."}`.
struct FileTokenSource {
    path: std::path::PathBuf,
}

#[async_trait]
impl TokenSourceFixed for FileTokenSource {
    async fn fetch(&self) -> TokenSourceResult<TokenSourceResponse> {
        let contents = std::fs::read_to_string(&self.path).map_err(serde_json::Error::io)?;
        Ok(serde_json::from_str(&contents)?)
    }
}
```

## Caching

Wrap any `TokenSourceConfigurable` with `.cached()` to reuse fetched credentials for repeat
fetches with equal options, for as long as the token has not expired:

```rust
let source = livekit_token_source::endpoint("https://example.com/api/token").cached();

let first = source.fetch(&options).await?;  // hits the endpoint
let second = source.fetch(&options).await?; // served from the cache
```

By default credentials are kept in memory and considered valid until the token's `exp` claim;
both are customizable via `with_store` (e.g. keychain- or database-backed persistence,
implementing the `TokenSourceStore` trait) and `with_validator`.

See [examples/token_source](../examples/token_source) for a runnable example.
