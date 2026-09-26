# LiveKit connection test

This CLI reports how the current machine connects to LiveKit. It identifies the selected WebRTC
path as one of:

- Direct UDP or TCP
- TURN over UDP, TCP, or TLS

The tool does not publish media and disables automatic track subscriptions.

## Run

From the repository root, provide a LiveKit URL and a participant token with permission to join a
room:

```bash
cargo run -p connection_test -- \
  --livekit-url wss://your-project.livekit.cloud \
  --livekit-token YOUR_TOKEN
```

## Example output

Direct connection:

```text
Connected to LiveKit.
  Connection: Direct UDP
  LiveKit endpoint: 203.0.113.10:50000/udp
```

Relayed connection:

```text
Connected to LiveKit.
  Connection: TURN/TLS
  TURN server: turns:turn.example.com:443
  LiveKit endpoint: 203.0.113.10:50000/udp
```

The command exits successfully after reporting the selected connection and closing the room.
