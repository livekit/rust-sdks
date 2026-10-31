# Session

A chat with a LiveKit agent, built on `livekit-session`. Lines typed on stdin are sent to the agent;
the agent's replies and state changes are printed as they arrive. By default no microphone is
published and the agent's audio is not played, so the agent needs to accept text input.

Credentials come from a [development token server](https://docs.livekit.io/frontends/build/authentication/development-token-server/),
which also dispatches the agent:

```bash
LIVEKIT_TOKEN_SERVER_ID=<token server id> LIVEKIT_AGENT_NAME=<agent name> cargo run -p session
```

Or from a pre-generated token, whose room configuration dispatches the agent:

```bash
LIVEKIT_URL=wss://<project>.livekit.cloud LIVEKIT_TOKEN=<token> cargo run -p session
```

Pass `--voice` to talk to the agent as well. The microphone is published through platform audio,
which also plays the agent's audio with echo cancellation, so no headphones are needed. Typed lines
are still sent, and what you say is printed once transcribed.

```bash
cargo run -p session -- --voice
```

Close stdin (Ctrl-D) to end the session.
