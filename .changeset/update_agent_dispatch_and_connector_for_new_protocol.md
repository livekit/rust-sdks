---
livekit-api: patch
livekit-protocol: patch
livekit-uniffi: patch
---

Update livekit-protocol submodule and propagate new fields:

- **livekit-api**: expose new agent dispatch and connector fields — `JobRestartPolicy` on dispatch types, `ringing_timeout` for `DialWhatsAppCall`/`AcceptWhatsAppCall`, `wait_until_answered` for `AcceptWhatsAppCall`, and a required `disconnect_reason` parameter on `disconnect_whatsapp_call`.
- **livekit-uniffi**: add `restart_policy` to `RoomAgentDispatch` and `tags` to `RoomConfiguration` to match the updated proto.
