This directory can contain a checkout of WebRTC from https://github.com/webrtc-sdk/webrtc. 
The build scripts here will install dependencies, checkout the version that LiveKit
currently uses, apply some patches to it, and build it. The currently used version is pinned in 
`.gclient`. For example, the build command to do a Linux debug build on x64 is:

```sh
$ ./build-linux.sh --arch x64 --profile release
```

After running this, `linux-x64-debug/lib/libwebrtc.a` should
exist. This can be rerun to rebuild it. Patches that are already applied
are skipped, but a patch that neither applies nor is already applied stops
the build, so a stale patch cannot produce a library that silently lacks it.

If something goes wrong it may be helpful to consult the [WebRTC native
development documentation](https://webrtc.googlesource.com/src/+/main/docs/native-code/development/).

# Building LiveKit Rust SDK with custom WebRTC checkout

Add the following environment variable to `/.config/config.toml`, to
specify use of a custom WebRTC build:

```toml
[env]
LK_CUSTOM_WEBRTC = { value = "webrtc-sys/libwebrtc/linux-x64-release", relative = true }
```

Note that `linux-x64-debug` should be replaced with the artifact
directory appropriate for your configuration.

# Downloading a custom WebRTC release

A release of the same libwebrtc built with other options (e.g. without the
bundled FFmpeg, or with an Objective-C class prefix) is downloaded as LiveKit's
is when `LK_WEBRTC_RELEASE` names its download URL. It must hold
`webrtc-{triple}.zip` archives laid out as LiveKit's releases, e.g.
`webrtc-linux-x64-release.zip` with `linux-x64-release/` at its root:

```toml
[env]
LK_WEBRTC_RELEASE = "https://github.com/<owner>/<repo>/releases/download/<tag>"
```
