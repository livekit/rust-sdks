# Basic Text Stream

Simple example using data streams to send text incrementally to all participants in a room.

## Usage

1. Connect to a room as a sender:

```sh
export LIVEKIT_URL="..."
export LIVEKIT_TOKEN="<first participant token>"
cargo run -- sender
```

2. Receive text streams from the first participant:

```sh
export LIVEKIT_URL="..."
export LIVEKIT_TOKEN="<second participant token>"
cargo run
```

3. Optionally run more senders and receivers (each must have a unique token).
