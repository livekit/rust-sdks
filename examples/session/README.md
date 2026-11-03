# Session

A conversation with a LiveKit agent, built on `livekit-session`. The microphone is published and
the agent's audio is played through platform audio, with echo cancellation, so no headphones are
needed. Lines typed on stdin are sent to the agent as well; the agent's replies, what you say once
transcribed, and state changes are printed as they arrive.

Credentials come from a [development token server](https://docs.livekit.io/frontends/build/authentication/development-token-server/),
which also dispatches the agent:

```bash
LIVEKIT_TOKEN_SERVER_ID=<token server id> LIVEKIT_AGENT_NAME=<agent name> cargo run -p session
```

Or from a pre-generated token, whose room configuration dispatches the agent:

```bash
LIVEKIT_URL=wss://<project>.livekit.cloud LIVEKIT_TOKEN=<token> cargo run -p session
```

Pass `--text` to leave the audio devices alone and talk by typing only, e.g. on a machine without
a microphone. The agent then needs to accept text input.

```bash
cargo run -p session -- --text
```

Close stdin (Ctrl-D) to end the session.
