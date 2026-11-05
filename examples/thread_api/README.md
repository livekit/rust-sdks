# Thread API

Lists the threads of one scope and prints the history of one thread, through the threads service
of a LiveKit Cloud project, with the `ThreadClient` of `livekit-api`. This is the read side of an
Agent 2.0 thread. A message to the agent goes to the agent endpoint, which this example does not do.

The credentials are the project's API key and secret. Run this on a developer machine only. Never
ship the key and secret in an app.

```bash
LIVEKIT_URL=wss://<project>.staging.livekit.cloud LIVEKIT_API_KEY=<key> LIVEKIT_API_SECRET=<secret> cargo run -p thread_api
```

`THREAD_SCOPE` selects the scope, `demo-user` by default, which is the scope the Agent 2.0 console
uses. `THREAD_ID` prints the history of that thread instead of the newest one.

Threads exist on LiveKit Cloud projects only. A local `livekit-server --dev` has no threads service.
