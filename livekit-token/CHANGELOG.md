# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.1 (2026-08-25)

### Fixes

#### Moves access-token generation and verification into a new `livekit-token` crate.

`livekit_api::access_token::*` continues to resolve to the same types via a
re-export, so no consumer changes are needed.

Also fixes the `services-tokio` and `services-async` features, which used the
access-token types without declaring the `access-token` feature. Building with
`--no-default-features --features services-tokio` previously failed to compile.

## 0.1.0

The initial release. This was broken out of [livekit-api](../livekit-api) into its own crate.
