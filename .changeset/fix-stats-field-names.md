---
libwebrtc: patch
---

Fix the serde keys for `InboundRtpStreamStats::total_freeze_duration` (`totalFreezesDuration`) and `OutboundRtpStreamStats::scalibility_mode` (`scalabilityMode`); both were always left at their defaults because the keys did not match what libwebrtc emits.
