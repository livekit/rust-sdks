# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.12.47](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.46...rust-sdks/livekit-ffi@0.12.47) - 2026-02-10

### Other

- update Cargo.toml dependencies
- don't use clamp as the ultimate_kbps can be lower than 300 ([#886](https://github.com/livekit/rust-sdks/pull/886))
- pre-connect the publisher PC when an RPC handler is registered ([#880](https://github.com/livekit/rust-sdks/pull/880))

## [0.12.46](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.45...rust-sdks/livekit-ffi@0.12.46) - 2026-02-09

### Other

- update Cargo.toml dependencies

## [0.12.45](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.44...rust-sdks/livekit-ffi@0.12.45) - 2026-02-09

### Fixed

- fix the RPC race condition ([#865](https://github.com/livekit/rust-sdks/pull/865))

### Other

- update proto & fix CI ([#871](https://github.com/livekit/rust-sdks/pull/871))
- add can_manage_agent_session permission ([#870](https://github.com/livekit/rust-sdks/pull/870))
- Use workspace dependencies & settings ([#856](https://github.com/livekit/rust-sdks/pull/856))
- Upgrade protocol to v1.44.0 ([#857](https://github.com/livekit/rust-sdks/pull/857))
- Use dedicated audio_runtime with high priority for audio capture  ([#854](https://github.com/livekit/rust-sdks/pull/854))
- Expose participant's permission to ffi layer ([#824](https://github.com/livekit/rust-sdks/pull/824))
- Add a request_async_id to the async requests ([#842](https://github.com/livekit/rust-sdks/pull/842))
- Use the correct download url in webrtc-sys build. ([#825](https://github.com/livekit/rust-sdks/pull/825))

## [0.12.44](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.43...rust-sdks/livekit-ffi@0.12.44) - 2026-01-15

### Fixed

- ensure Room.creation_time is ms ([#822](https://github.com/livekit/rust-sdks/pull/822))

### Other

- try setting x-google-start-bitrate for vp9 ([#820](https://github.com/livekit/rust-sdks/pull/820))

## [0.12.43](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.42...rust-sdks/livekit-ffi@0.12.43) - 2025-12-19

### Added

- *(ParticipantInfo)* export kind details ([#813](https://github.com/livekit/rust-sdks/pull/813))

## [0.12.42](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.41...rust-sdks/livekit-ffi@0.12.42) - 2025-12-17

### Other

- Handle server initiated mute request ([#812](https://github.com/livekit/rust-sdks/pull/812))

## [0.12.41](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.40...rust-sdks/livekit-ffi@0.12.41) - 2025-12-04

### Added

- *(connector)* initial service impl ([#790](https://github.com/livekit/rust-sdks/pull/790))

### Fixed

- fix mute/unmute events for LocalTrack. ([#799](https://github.com/livekit/rust-sdks/pull/799))

### Other

- Add RoomEvent::TokenRefreshed ([#803](https://github.com/livekit/rust-sdks/pull/803))
- Expose desktop capturer ([#725](https://github.com/livekit/rust-sdks/pull/725))

## [0.12.40](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.39...rust-sdks/livekit-ffi@0.12.40) - 2025-11-20

### Other

- change the livekit-ffi to output a static lib for cpp SDK ([#781](https://github.com/livekit/rust-sdks/pull/781))
- perform full reconnect if resume fails ([#792](https://github.com/livekit/rust-sdks/pull/792))
- E2E RPC tests ([#769](https://github.com/livekit/rust-sdks/pull/769))
- Remove unused dependencies ([#761](https://github.com/livekit/rust-sdks/pull/761))

## [0.12.39](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.38...rust-sdks/livekit-ffi@0.12.39) - 2025-10-27

### Fixed

- fix unable to locate __arm_tpidr2_save for android ffi. ([#765](https://github.com/livekit/rust-sdks/pull/765))

### Other

- Linux hardware acceleration build fixes ([#753](https://github.com/livekit/rust-sdks/pull/753))
- Expose set video quality ([#759](https://github.com/livekit/rust-sdks/pull/759))

## [0.12.38](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.37...rust-sdks/livekit-ffi@0.12.38) - 2025-10-23

### Other

- add h265 codec support ([#762](https://github.com/livekit/rust-sdks/pull/762))

## [0.12.37](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.36...rust-sdks/livekit-ffi@0.12.37) - 2025-10-22

### Other

- License check ([#746](https://github.com/livekit/rust-sdks/pull/746))
- Derive from_variants for FFI oneof fields ([#738](https://github.com/livekit/rust-sdks/pull/738))
- Remove participant check for data packets ([#757](https://github.com/livekit/rust-sdks/pull/757))
- clamp connection timeout and fixed the comment ([#748](https://github.com/livekit/rust-sdks/pull/748))
- put examples in root Cargo workspace ([#731](https://github.com/livekit/rust-sdks/pull/731))

## [0.12.36](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.35...rust-sdks/livekit-ffi@0.12.36) - 2025-10-13

### Added

- *(e2ee)* add data channel encryption ([#708](https://github.com/livekit/rust-sdks/pull/708))

### Fixed

- fix some potential audio issues, clean up the code a bit, and suppress some warnings  ([#737](https://github.com/livekit/rust-sdks/pull/737))
- do not log 'signal client closed: "stream closed"' on disconnect ([#727](https://github.com/livekit/rust-sdks/pull/727))

### Other

- Upgrade prost, use prost-build (FFI only) ([#734](https://github.com/livekit/rust-sdks/pull/734))
- Test participant disconnect ([#732](https://github.com/livekit/rust-sdks/pull/732))
- Increase RPC max RT time to 7s ([#729](https://github.com/livekit/rust-sdks/pull/729))
- E2E audio test ([#724](https://github.com/livekit/rust-sdks/pull/724))
- bump libwebrtc libs version for webrtc-sys. ([#741](https://github.com/livekit/rust-sdks/pull/741))
- Bump reqwest to 0.12 ([#711](https://github.com/livekit/rust-sdks/pull/711))

## [0.12.35](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.34...rust-sdks/livekit-ffi@0.12.35) - 2025-10-03

### Other

- updated the following local packages: webrtc-sys, livekit

## [0.12.34](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.33...rust-sdks/livekit-ffi@0.12.34) - 2025-09-29

### Fixed

- apply original participant fields in data messages ([#709](https://github.com/livekit/rust-sdks/pull/709))

### Other

- Add send_bytes method ([#691](https://github.com/livekit/rust-sdks/pull/691))
- Implement Display and Error for RpcError ([#719](https://github.com/livekit/rust-sdks/pull/719))
- Fix intermittently failing E2E reliability test ([#718](https://github.com/livekit/rust-sdks/pull/718))
- Do not modify raw packets ([#714](https://github.com/livekit/rust-sdks/pull/714))
- Disable opus red for e2ee enabled clients ([#706](https://github.com/livekit/rust-sdks/pull/706))
- Upgrade protocol to v1.41.0 ([#703](https://github.com/livekit/rust-sdks/pull/703))
- Upgrade libwebrtc to m137. ([#696](https://github.com/livekit/rust-sdks/pull/696))

## [0.12.33](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.32...rust-sdks/livekit-ffi@0.12.33) - 2025-09-09

### Other

- Add option to enable audio preconnect buffer in SDK & FFI ([#700](https://github.com/livekit/rust-sdks/pull/700))
- Optional flags for video hw codec. ([#701](https://github.com/livekit/rust-sdks/pull/701))
- Data channel reliability ([#688](https://github.com/livekit/rust-sdks/pull/688))

## [0.12.32](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.31...rust-sdks/livekit-ffi@0.12.32) - 2025-09-03

### Other

- updated the following local packages: livekit-api, livekit

## [0.12.31](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.30...rust-sdks/livekit-ffi@0.12.31) - 2025-07-31

### Other

- updated the following local packages: livekit-api, livekit
# Changelog

## [0.12.30](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.29...rust-sdks/livekit-ffi@0.12.30) - 2025-07-18

### Fixed

- fix SoxrResampler flush segv ([#678](https://github.com/livekit/rust-sdks/pull/678))

## [0.12.29](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.28...rust-sdks/livekit-ffi@0.12.29) - 2025-07-16

### Other

- remove published tracks when the room is closed ([#677](https://github.com/livekit/rust-sdks/pull/677))

## [0.12.28](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.27...rust-sdks/livekit-ffi@0.12.28) - 2025-06-23

### Fixed

- `audio_frame_ms` didn't work expectedly ([#671](https://github.com/livekit/rust-sdks/pull/671))

## [0.12.27](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.26...rust-sdks/livekit-ffi@0.12.27) - 2025-06-17

### Fixed

- *(webrtc-sys-build)* add temporary workaround to fix ci in Windows ([#665](https://github.com/livekit/rust-sdks/pull/665))
- *(webrtc-sys-build)* add error context to debug issues ([#664](https://github.com/livekit/rust-sdks/pull/664))

### Other

- Expose room updates, support MoveParticipant (protocol 15) ([#662](https://github.com/livekit/rust-sdks/pull/662))
- use path.join instead of hardcoded `/` ([#663](https://github.com/livekit/rust-sdks/pull/663))
- bump version for webrtc (fix win CI) ([#650](https://github.com/livekit/rust-sdks/pull/650))
- try to fix webrtc build for iOS/macOS. ([#646](https://github.com/livekit/rust-sdks/pull/646))
- remove ([#633](https://github.com/livekit/rust-sdks/pull/633))

## [0.12.20] - 2025-04-08

### Added

- FFI support for the new high-level data streams API

## [0.12.18] - 2025-03-21

### Fixed

- Fix several minor issues
- Revert the RPC change, need more robust way

## [0.12.17] - 2025-03-18

### Fixed

- Move RPC handlers to room

## [0.12.16] - 2025-03-06

### Fixed

- Fixed metric report issue on audio filter where room_id is sometimes empty

## [0.12.15] - 2025-03-05

### Fixed

- Ensure RTC session continues even if audio filter initialization fails

## [0.12.14] - 2025-03-04

### Added

- APM

### Fixed

- debugging nanpa
- Fixed a packaging issue

## [0.12.13] - 2025-03-04

### Fixed

- Depends the latest version of livekit-protocol

## [0.12.12] - 2025-02-28

### Added

- Add audio filter support

## [0.12.11] - 2025-02-24

### Changed

- fixed passing disconnect reason on ParticipantDisconnected events

## [0.12.10] - 2025-02-04

### Fixed

- Fix RPC invocation race bug

## [0.12.9] - 2025-02-03

### Added

- Support for track subscription permissions

## [0.12.8] - 2025-01-23

### Changed

- Rename DataStream header properties

## [0.12.7] - 2025-01-17

### Added

- Add DataStream.Trailer support
- Add an API to set buffer_amount_low_threshold for DataChannel
- Update RoomInfo to contain buffer_amount_low_threshold for DataChannel

## [0.12.6] - 2025-01-07

### Fixed

- Automatically close audio/video stream handles when the associated room is closed

## [0.12.5] - 2025-01-04

### Added

- Fix deadlock issue in nested RPC calls.

## [0.12.4] - 2024-12-22

### Added

- bump libwebrtc to m125

## [0.12.3] - 2024-12-14

### Added

- bump libwebrtc to m125
## 0.12.49 (2026-02-23)

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
- add error context to debug issues (#664)
- add temporary workaround to fix ci in Windows (#665)
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
