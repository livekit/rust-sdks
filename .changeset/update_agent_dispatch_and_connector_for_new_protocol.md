---
livekit-api: patch
livekit-protocol: patch
---

Update livekit-protocol submodule and expose new agent dispatch and connector fields in livekit-api: `JobRestartPolicy` on dispatch types, `ringing_timeout` for `DialWhatsAppCall`/`AcceptWhatsAppCall`, `wait_until_answered` for `AcceptWhatsAppCall`, and a required `disconnect_reason` parameter on `disconnect_whatsapp_call`.
