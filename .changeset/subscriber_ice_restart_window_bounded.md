---
livekit: patch
---

Fix the subscriber buffering remote ICE candidates for the rest of the session after a resume.

A resume marked the subscriber as awaiting a fresh ICE generation so that remote candidates
queue rather than being applied to a generation on its way out, but only an arriving remote
description closed that window — and the server re-offers the subscriber only when the resume
moved the participant to a different node. After an ordinary signal-only resume no offer
arrives, so the window stayed open and every later remote candidate was queued instead of
applied, leaving the subscriber unable to adopt any new network path the server proposed. The
window is now closed on every exit from the resume, applying anything queued behind it.
