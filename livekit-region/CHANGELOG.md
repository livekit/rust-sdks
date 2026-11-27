# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.2

### Patch Changes

- Add `Room::prepare_connection` and the FFI `PrepareConnectionRequest`, which pick the best region of a LiveKit Cloud project before connecting and keep the region list fresh until then, so that the next connect to the same URL dials that region directly; `Room::cancel_prepared_connection` undoes it, and `livekit-signaling` adds `RegionUrlProvider::prepare` and `RegionUrlProvider::cancel_prepared` for the same purpose. - [#1711](https://github.com/livekit/rust-sdks/pull/1711) ([@lukasIO](https://github.com/lukasIO))
- A connected signal session on a LiveKit Cloud URL now refetches the region list whenever it goes stale, using the latest token, so a region failover after a long call does not wait for a fetch. - [#1710](https://github.com/livekit/rust-sdks/pull/1710) ([@lukasIO](https://github.com/lukasIO))

## 0.1.1 (2026-09-08)

### Fixes

#### Moves the internal region-discovery cache into a new `livekit-region` crate. No

public API or behaviour change.

## 0.1.0

The initial release. This was broken out of [livekit-api](../livekit-api) into its own crate.
