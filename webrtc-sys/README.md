# webrtc-sys

This crate provides wrapper over the WebRTC API for use from Rust.
We use the crate [cxx.rs](https://cxx.rs/) to simplify our bindings.

## Wrappers

Most of your wrappers are just used to use the cxx.rs types and use them from Rust.
As all wrappers are stateless, we allow multiple instances of a specific wrapper to point to the same underlying webrtc pointer. (e.g: multiple livekit::MediaStreamTrack pointing to the same webrtc::MediaStreamTrackInterface).

Threadsafe methods use the const keyword so we can easily call them from the Rust side without worrying about the mutability of the object. (This is similar on how Cell/UnsafeCell works but implemented on the C++ side: interior Mutability).

## Code

We also use this C++ code to provide other needed utilities/features on the Rust side (e.g: tiny bindings to libyuv).

