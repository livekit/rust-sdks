# Basic Data Track

Simple example of publishing and subscribing to a data track.

## Usage

1. Run the publisher:

```sh
export LIVEKIT_URL="..."
export LIVEKIT_TOKEN="<first participant token>"
cargo run --bin publisher
```

2. In a second terminal, run the subscriber:

```sh
export LIVEKIT_URL="..."
export LIVEKIT_TOKEN="<second participant token>"
cargo run --bin subscriber
```
