# Agent Audio Latency Example

This example connects to a LiveKit room, publishes microphone audio with `cpal`, plays remote audio back to the default speaker, and optionally injects a short probe tone to estimate agent response latency.

It is audio-only. No video tracks are created.

## What the latency metric means

When `--benchmark` is enabled, the app waits for remote audio to be quiet, injects a short tone burst into the outgoing audio, and measures how long it takes before remote audio becomes active again.

That metric works well for:

- echo / loopback agents
- agents that immediately answer with speech or audio

It is not a codec-level mouth-to-ear measurement. It is an application-level "probe sent to first remote audio response" estimate.

## Usage

With a pre-minted participant token:

```bash
cargo run -p agent_audio_latency -- \
  --url "$LIVEKIT_URL" \
  --token "$LIVEKIT_TOKEN"
```

Or mint a token locally:

```bash
cargo run -p agent_audio_latency -- \
  --url "$LIVEKIT_URL" \
  --api-key "$LIVEKIT_API_KEY" \
  --api-secret "$LIVEKIT_API_SECRET" \
  --room-name my-room \
  --identity rust-agent-client
```

Enable the latency benchmark:

```bash
cargo run -p agent_audio_latency -- \
  --url "$LIVEKIT_URL" \
  --token "$LIVEKIT_TOKEN" \
  --benchmark
```

If you only want to listen to a specific agent participant:

```bash
cargo run -p agent_audio_latency -- \
  --url "$LIVEKIT_URL" \
  --token "$LIVEKIT_TOKEN" \
  --agent-identity my-agent
```
