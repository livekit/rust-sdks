---
livekit-session: patch
---

Add `Session::subscribe_with_messages`, which returns the message feed and a receiver of its later changes together, so an app can show the feed and then follow it without a change falling between the two.
