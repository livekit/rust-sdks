# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.1 (2026-09-04)

### Features

- Removes livekit-runtime and converts this package to be tokio only again - #1375 (@1egoman)

### Fixes

- Add data streams v2 to exposed uniffi interface - #1286 (@1egoman)

#### Moves the signalling client into a new `livekit-signaling` crate. livekit-api

re-exports it under the historical `livekit_api::signal_client` path, now marked
deprecated: it is internal SDK API, and dependents should use livekit-signaling
directly. livekit-api no longer depends on livekit-net.

Also drops two dependencies that were declared but never used: `scopeguard` and
`bytes`.

## 0.1.0

The initial release. This was broken out of [livekit-api](../livekit-api) into its own crate.
