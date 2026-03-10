---
webrtc-sys: patch
---

# avoid getting webrtc into underrun

Before this change, the Rust implementation would only start sending silence frames after missing 10 consecutive audio frames. This could cause WebRTC's audio pipeline to enter an underrun state when audio stopped temporarily.

Once WebRTC enters underrun, resuming audio can significantly increase latency until the pipeline stabilizes again. In testing, this could add hundreds of milliseconds of additional latency when audio resumed shortly after the underrun.

This change ensures silence frames are sent earlier to prevent the audio pipeline from entering underrun. By maintaining a continuous stream of audio (including silence), WebRTC can avoid unnecessary buffering and latency spikes when audio resumes.

In testing, this reduces the additional latency observed after underrun recovery and results in more stable end-to-end audio latency.
