# Session

A text-only chat with a LiveKit agent, built on `livekit-session`. Lines typed on stdin are sent to
the agent; the agent's replies and state changes are printed as they arrive. No microphone is
published, so the agent needs to accept text input.

Credentials come from a [sandbox token server](https://docs.livekit.io/frontends/build/authentication/sandbox-token-server/),
which also dispatches the agent:

```bash
LIVEKIT_SANDBOX_ID=<sandbox id> LIVEKIT_AGENT_NAME=<agent name> cargo run -p session
```

Or from a pre-generated token, whose room configuration dispatches the agent:

```bash
LIVEKIT_URL=wss://<project>.livekit.cloud LIVEKIT_TOKEN=<token> cargo run -p session
```

Close stdin (Ctrl-D) to end the session.
