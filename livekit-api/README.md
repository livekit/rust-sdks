# LiveKit Server APIs

The official server API crate for [LiveKit](https://livekit.com).

Use this crate to generate access tokens and invoke LiveKit server APIs for rooms, egress, ingress, SIP, agent dispatch, and more.

## Server API

`LiveKitApi` is a single entry point to every server API, exposing each service through an accessor (`room()`, `egress()`, `ingress()`, `sip()`, `agent_dispatch()`, `connector()`).

```rust,no_run
use livekit_api::services::LiveKitApi;
use livekit_api::services::room::CreateRoomOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lk = LiveKitApi::with_api_key("https://my.livekit.host", "my-key", "my-secret");

    let room = lk
        .room()
        .create_room(
            "my-room",
            CreateRoomOptions { empty_timeout: 600, max_participants: 20, ..Default::default() },
        )
        .await?;

    println!("created room {}", room.name);
    Ok(())
}
```

Individual service clients (`RoomClient`, `SIPClient`, etc.) can also be created directly with the same constructors.

### Authentication

The server API supports two modes of operation:

- **API key & secret** — recommended for backend use. `LiveKitApi::with_api_key(host, key, secret)` signs a short-lived token for each request. `LiveKitApi::new(host)` reads the key and secret from the `LIVEKIT_API_KEY` and `LIVEKIT_API_SECRET` environment variables.
- **Access token** — for client-side use where the API secret must not be exposed. `LiveKitApi::with_token(host, token)` sends a pre-signed [access token](https://docs.livekit.io/frontends/reference/tokens-grants/) verbatim on every request; its grants must cover the calls you make.

### Agent dispatch

Explicitly dispatch an agent into a room (see [Agent dispatch](https://docs.livekit.io/agents/server/agent-dispatch/)):

```rust,no_run
use livekit_api::services::LiveKitApi;
use livekit_protocol as proto;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lk = LiveKitApi::with_api_key("https://my.livekit.host", "my-key", "my-secret");

    lk.agent_dispatch()
        .create_dispatch(proto::CreateAgentDispatchRequest {
            room: "my-room".to_owned(),
            agent_name: "my-agent".to_owned(),
            ..Default::default()
        })
        .await?;
    Ok(())
}
```

### Error handling

Service methods return `ServiceResult<T>` (`Result<T, ServiceError>`). A failed server call is a `ServiceError::Twirp(ServerError)`; when the server returns a structured error it is `ServerError::Twirp(ServerErrorCode)`, which carries the error code and message. (`ServerError`'s former `TwirpError`/`TwirpErrorCode`/`TwirpResult` type names remain as deprecated aliases.)

```rust,no_run
use livekit_api::services::LiveKitApi;

#[tokio::main]
async fn main() {
    let lk = LiveKitApi::with_api_key("https://my.livekit.host", "my-key", "my-secret");
    match lk.room().delete_room("my-room").await {
        Ok(_) => {}
        Err(e) => eprintln!("delete_room failed: {e}"),
    }
}
```

### Handling SIP call errors

When a SIP call fails (e.g. the callee is busy or declines), the server attaches a SIP status to the error. `SipCallError::from_error` decodes it from a returned error, exposing the SIP status code and reason:

```rust,no_run
use livekit_api::services::sip::CreateSIPParticipantOptions;
use livekit_api::services::{LiveKitApi, SipCallError};

#[tokio::main]
async fn main() {
    let lk = LiveKitApi::with_api_key("https://my.livekit.host", "my-key", "my-secret");

    let result = lk
        .sip()
        .create_sip_participant(
            "ST_trunk".to_owned(),
            "+15105550100".to_owned(),
            "my-room".to_owned(),
            CreateSIPParticipantOptions {
                wait_until_answered: Some(true),
                ..Default::default()
            },
            None,
        )
        .await;

    if let Err(err) = result {
        if let Some(sip) = SipCallError::from_error(&err) {
            eprintln!("{sip}"); // e.g. "SIP call failed: 486 Busy Here (resource_exhausted)"
            if sip.sip_status_code() == Some(486) {
                // callee is busy
            }
        }
    }
}
```

## Access tokens

Access tokens are generated with `AccessToken`:

```rust,no_run
use livekit_api::access_token::{AccessToken, VideoGrants};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = AccessToken::with_api_key("my-key", "my-secret")
        .with_identity("participant-identity")
        .with_name("Participant Name")
        .with_grants(VideoGrants {
            room_join: true,
            room: "my-room".to_owned(),
            ..Default::default()
        })
        .to_jwt()?;

    println!("{token}");
    Ok(())
}
```

By default, tokens expire 6 hours after generation. Override this with `.with_ttl(duration)`.
