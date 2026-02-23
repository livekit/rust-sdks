# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.13](https://github.com/livekit/rust-sdks/compare/rust-sdks/yuv-sys@0.3.12...rust-sdks/yuv-sys@0.3.13) - 2026-02-10

### Other

- update libyuv & support neon simd ([#882](https://github.com/livekit/rust-sdks/pull/882))

## [0.3.12](https://github.com/livekit/rust-sdks/compare/rust-sdks/yuv-sys@0.3.11...rust-sdks/yuv-sys@0.3.12) - 2026-02-09

### Other

- mark jpeg dependency optional ([#874](https://github.com/livekit/rust-sdks/pull/874))

## [0.3.11](https://github.com/livekit/rust-sdks/compare/rust-sdks/yuv-sys@0.3.10...rust-sdks/yuv-sys@0.3.11) - 2026-02-09

### Other

- Use workspace dependencies & settings ([#856](https://github.com/livekit/rust-sdks/pull/856))
- Add local video publisher & subcriber examples ([#830](https://github.com/livekit/rust-sdks/pull/830))

## [0.3.10](https://github.com/livekit/rust-sdks/compare/rust-sdks/yuv-sys@0.3.9...rust-sdks/yuv-sys@0.3.10) - 2025-10-22

### Other

- License check ([#746](https://github.com/livekit/rust-sdks/pull/746))
- Cargo workspace cleanup ([#745](https://github.com/livekit/rust-sdks/pull/745))

## [0.3.9](https://github.com/livekit/rust-sdks/compare/rust-sdks/yuv-sys@0.3.8...rust-sdks/yuv-sys@0.3.9) - 2025-09-29

### Other

- Upgrade libwebrtc to m137. ([#696](https://github.com/livekit/rust-sdks/pull/696))

## [0.3.8](https://github.com/livekit/rust-sdks/compare/rust-sdks/yuv-sys@0.3.7...rust-sdks/yuv-sys@0.3.8) - 2025-09-09

### Other

- Optional flags for video hw codec. ([#701](https://github.com/livekit/rust-sdks/pull/701))
# Changelog

## [0.3.7] - 2024-12-14

### Added

- move yuv-sys to main rust-sdks monorepo

### Fixed

- fix yuv simd errors
## 0.3.14 (2026-02-23)

### Features

- basic ffi server to support other languages (#34)
- video publishing (#42)
- audio support (#45)
- server sdk (#47)
- use new webrtc builds & linux support  (#54)
- add publishing & audio to ffi (#46)
- add cbindgen to livekit-ffi (#57)
- update simple_room wgpu (#61)
- add a basic_room demo (#62)
- AudioResampler (#66)
- forward libwebrtc logs (#75)
- tls features  (#86)
- add ffi datachannel & mute events (#88)
- webrtc builds for ios & android (#93)
- rtc_use_h264 on windows & linux (#94)
- move signal_client to livekit-api (#98)
- use new webrtc builds (#103)
- ffi data received event (#109)
- optimize livekit-ffi for size (#111)
- include libwebrtc.jar on webrtc builds (#116)
- generate proto ahead of time (#117)
- ios & android support (#106)
- android hw mediacodec (#119)
- android with libc++_static (#123)
- change ffi proto package (#124)
- warn blocking callbacks (#125)
- use libwebrtc m114 (#130)
- wgpu_room improvements (#131)
- support more events on the ffi (#133)
- support webhooks for livekit-api (#99)
- add disconnect reason (#137)
- update pc configs on signal reconnect (#138)
- call rtc/validate endpoint on signal failures (#139)
- debounce publisher negotiations (#136)
- queue signal messages on reconnection (#140)
- ffi improvements (#141)
- sync states after resume (#144)
- republish tracks on signal restart (#145)
- end-to-end encryption (#161)
- more debugging logs (#176)
- add eos events (#184)
- automatically switch to ws protocol when using http (#186)
- update dependencies & rename livekit-webrtc to libwebrtc (#195)
- add more logs & monitor stuck tasks  (#197)
- room eos (#198)
- automatically release drafts for ffi & add license  (#199)
- add RtcConfiguration to RoomOptions (#200)
- handle metadata (#205)
- ffi update metadata requests (#209)
- handle ping & correctly close the signal_task (#223)
- allow conversions using ptr on the ffi (#226)
- rtc stats (#218)
- stats request (#229)
- initial connection retry (#245)
- ffi logger (#246)
- suport topic on data packets (#256)
- add svc codecs (#263)
- session stats (#266)
- Audio Filter plugin (#559)
- Region pinning support (#631)
- VA-API support for linux. (#638)
- add outbound trunk config for create_sip_participant. (#771)

### Fixes

- use the new event system on the demo (#10)
- windows crashes & builds (#55)
- cbindgen extern fnc (#58)
- incorrect resample response (#67)
- room_join should be false by default on access tokens (#70)
- different audio frame size than 10ms (#72)
- multiple audio channels (#73)
- audio data loss (#74)
- linux ssl connections crashes (#79)
- linux & win builds (#84)
- wrong build target on ffi builds (#85)
- ffi builds output
- datachannel deadlocks & dispose crashes (#87)
- use native tls roots on ffi (#91)
- webrtc licenses generation (#95)
- licenses patch
- corrupt patch
- unsubscribe deadlock (#101)
- publish track request on the ffi
- add missing licenses & description
- win-aarch64 ffi builds (#107)
- correctly wait publisher connection (#108)
- race on WebRtcVoiceEngine dispose (#112)
- adm (#115)
- desktop builds (#120)
- android builds (#121)
- android crashes + working example (#122)
- signal_stream send panic (#127)
- android ffi binary size (#128)
- webrtc m114 builds (#129)
- manual subscription (#132)
- simulcast encoder & bitrate tweaks (#134)
- undefined symbols for simulcast adapter (#135)
- synchronise publish events on the ffi (#146)
- missing participants_info
- build ffi on older linux machines (#147)
- ubuntu2204 for arm
- more ffi client synchronisation (#148)
- unpublish track & initial connect event on the ffi (#149)
- forgot to call unpublish (#150)
- is_subscribed returning false when using auto_subscribe (#151)
- regression set_subscribed request (#153)
- portable ffi download script (#160)
- macosx_deployment_target to 10.15 (#165)
- muted events & capture_audio response (#171)
- no tokio-reactor on ffi audio capture (#172)
- create audio_source interval in an async context (#173)
- tracks with no e2ee (#175)
- wrong ice_restart value (#178)
- cancelled negotation (#180)
- negotiate only once (#179)
- correctly simulate force tcp/tls (#182)
- publisher migration failures (#187)
- e2ee and upgrade webrtc (#190)
- support non-utf8 std::string which can come e.g. from Windows message errors (#193)
- docs.rs builds (#194)
- remote track desired state (#196)
- participant updates (#202)
- stuck tasks while closing the PCs (#203)
- better reconnection logic & safety (#204)
- audio source captures with "late" frames  (#207)
- wrong queuable messages (#206)
- data from the server sdk (#212)
- published_track order (#214)
- relative /rtc & remove sensitive logs (#220)
- swallow data when participant is invalid (#222)
- redundant close on SignalClient (#224)
- invalid stats panic (#231)
- appropriate encoding (#265)
- do nothing when already subscribed (#324)
- don't ignore encodings from TrackPublishOptions (#348)
- simplified `enable_queue` field for AudioSource. (#360)
- resume/reconnection not working! (#440)
- avoid wrong reconnection logs  (#441)
- don't overwrite the url path in twirp-client (#478)
- re-export twirp error types (#480)
- Doc.rs Build Fails For livekit-api = 0.4.1 (#495)
- webrtc builds on macOS, iOS (#514)
- pass disconnect reason explicitly (#581)
- `audio_frame_ms` didn't work expectedly (#671)
- hardware rendering (#695)
- apply original participant fields in data messages (#709)
- change search_dirs to use cc --print-search-dirs instead of clang --print-search-dirs (#697)
- do not log 'signal client closed: "stream closed"' on disconnect (#727)
- fix linux so link issue. (#733)
- fix unable to locate __arm_tpidr2_save for android ffi. (#765)
- fix mute/unmute events for LocalTrack. (#799)
- Update driver installation step with more dependencies for release-plz (#807)
- lazy loading for additional dependencies. (#814)
- Exclude the desktop-capturer module link for mobile. (#817)
- ensure Room.creation_time is ms (#822)
