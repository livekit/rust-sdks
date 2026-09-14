---
livekit: patch
livekit-ffi: patch
livekit-capture: patch
---

Write the `x-google-start-bitrate` hint once per publisher connection, and exempt screen share from the 1 Mbps cap.

libwebrtc reads this fmtp parameter per m-section but applies it to the shared `Call` (`WebRtcVideoSendChannel::ApplyChangedParams` -> `SetSdpBitrateParameters`), where `RtpBitrateConfigurator` holds one config for the whole peer connection. It retains `start_bitrate_bps` and re-applies it on network route changes (`RtpTransportControllerSend::OnNetworkRouteChanged`), so a WiFi-to-cellular handover re-seeds the estimator from the original hint with no renegotiation. Rewriting the value on later offers was therefore at best a no-op and at worst a restart of a converged bandwidth estimator; it is now written only on the first offer that carries local video, and only once that offer is accepted locally. A full reconnect builds a new peer connection and seeds the new estimator again.

Screen share is no longer capped at 1 Mbps, matching client-sdk-js and client-sdk-android: unlike camera content, a screen share is published at a high bitrate so text stays legible, and a conservative start costs more than a brief overshoot.
