// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! RTSP test helpers: an in-process GStreamer RTSP server and the encoder
//! pipelines it serves.

use std::{
    thread,
    time::{Duration, Instant},
};

use gstreamer::{self as gst, glib};
use gstreamer_rtsp_server::{prelude::*, RTSPAuth, RTSPMediaFactory, RTSPServer, RTSPToken};
use livekit_capture::sources::rtsp::RtspVideoSourceConfig;

/// Test streams are 640x480 at 30 fps with a keyframe every 30 frames.
pub const TEST_WIDTH: u32 = 640;
pub const TEST_HEIGHT: u32 = 480;

pub const H264_PIPELINE: &str = "videotestsrc is-live=true \
    ! video/x-raw,width=640,height=480,framerate=30/1 ! videoconvert \
    ! x264enc tune=zerolatency speed-preset=ultrafast key-int-max=30 bitrate=500 \
      byte-stream=true aud=true \
    ! h264parse config-interval=-1 ! rtph264pay name=pay0 pt=96 config-interval=1";

pub const H265_PIPELINE: &str = "videotestsrc is-live=true \
    ! video/x-raw,width=640,height=480,framerate=30/1 ! videoconvert \
    ! x265enc tune=zerolatency speed-preset=ultrafast key-int-max=30 bitrate=500 \
      option-string=repeat-headers=1:aud=1:open-gop=0 \
    ! h265parse config-interval=-1 ! rtph265pay name=pay0 pt=96 config-interval=1";

pub const VP8_PIPELINE: &str = "videotestsrc is-live=true \
    ! video/x-raw,width=640,height=480,framerate=30/1 ! videoconvert \
    ! vp8enc deadline=1 cpu-used=8 keyframe-max-dist=30 lag-in-frames=0 \
      target-bitrate=500000 \
    ! rtpvp8pay name=pay0 pt=96";

/// An in-process GStreamer RTSP server serving one launch pipeline at
/// `/test` on an ephemeral localhost port.
pub struct RtspTestServer {
    main_loop: glib::MainLoop,
    thread: Option<thread::JoinHandle<()>>,
    port: i32,
}

impl RtspTestServer {
    /// Starts a server streaming `media_pipeline`, which must end in an RTP
    /// payloader named `pay0`.
    pub fn launch(media_pipeline: &str) -> Self {
        Self::launch_inner(media_pipeline, None)
    }

    /// Starts a server that requires Digest authentication.
    pub fn launch_with_digest_auth(
        media_pipeline: &str,
        username: &str,
        password: &str,
    ) -> Self {
        Self::launch_inner(media_pipeline, Some((username, password)))
    }

    fn launch_inner(media_pipeline: &str, digest: Option<(&str, &str)>) -> Self {
        gst::init().expect("failed to initialize GStreamer");

        // Each server runs on its own main context so parallel tests never
        // contend for the default one.
        let context = glib::MainContext::new();
        let server = RTSPServer::new();
        server.set_address("127.0.0.1");
        // Bind an ephemeral port; the real port is read back after attach.
        server.set_service("0");

        let factory = RTSPMediaFactory::new();
        factory.set_launch(&format!("( {media_pipeline} )"));
        factory.set_shared(false);

        if let Some((username, password)) = digest {
            factory.add_role_from_structure(
                &gst::Structure::builder("user")
                    .field("media.factory.access", true)
                    .field("media.factory.construct", true)
                    .build(),
            );
            let token = RTSPToken::builder().field("media.factory.role", "user").build();
            let auth = RTSPAuth::new();
            auth.set_supported_methods(gstreamer_rtsp::RTSPAuthMethod::Digest);
            auth.add_digest(username, password, &token);
            server.set_auth(Some(&auth));
        }

        server
            .mount_points()
            .expect("RTSP server has no mount points")
            .add_factory("/test", factory);
        server.attach(Some(&context)).expect("failed to attach RTSP server");
        let port = server.bound_port();
        assert!(port > 0, "RTSP server reported no bound port");
        log::info!(
            "RTSP test server listening at rtsp://127.0.0.1:{port}/test{}",
            if digest.is_some() { " (digest auth)" } else { "" },
        );
        log::debug!("RTSP test server pipeline: {media_pipeline}");

        let main_loop = glib::MainLoop::new(Some(&context), false);
        let run_loop = main_loop.clone();
        let thread = thread::Builder::new()
            .name("rtsp-test-server".to_owned())
            .spawn(move || run_loop.run())
            .expect("failed to spawn RTSP server thread");

        // A quit before the loop runs would be lost; wait for it to start.
        let started = Instant::now();
        while !main_loop.is_running() {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "RTSP server main loop did not start"
            );
            thread::sleep(Duration::from_millis(1));
        }

        Self { main_loop, thread: Some(thread), port }
    }

    /// The stream's RTSP URL.
    pub fn url(&self) -> String {
        format!("rtsp://127.0.0.1:{}/test", self.port)
    }
}

impl Drop for RtspTestServer {
    fn drop(&mut self) {
        self.main_loop.quit();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A source configuration for `url` with test-friendly timeouts and every
/// optional field unset.
pub fn test_config(url: String) -> RtspVideoSourceConfig {
    RtspVideoSourceConfig {
        url,
        username: None,
        password: None,
        codec: None,
        resolution: None,
        connect_timeout_ms: Some(5_000),
        idle_timeout_ms: Some(10_000),
    }
}
