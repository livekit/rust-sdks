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
use livekit_capture::{primitive::VideoResolution, sources::rtsp::RtspVideoSourceConfig};

/// Default test streams are 640x480 at 30 fps with a keyframe every 30
/// frames.
pub const TEST_RESOLUTION: VideoResolution = VideoResolution::new(640, 480);

/// Codecs the test server can encode.
#[derive(Debug, Clone, Copy)]
pub enum TestCodec {
    H264,
    H265,
    Vp8,
    Vp9,
    Av1,
}

/// Builds an encoder pipeline for the test server at the given resolution.
pub fn pipeline(codec: TestCodec, resolution: VideoResolution) -> String {
    let encode = match codec {
        TestCodec::H264 => {
            "x264enc tune=zerolatency speed-preset=ultrafast key-int-max=30 bitrate=500 \
             byte-stream=true aud=true \
             ! h264parse config-interval=-1 ! rtph264pay name=pay0 pt=96 config-interval=1"
        }
        TestCodec::H265 => {
            "x265enc tune=zerolatency speed-preset=ultrafast key-int-max=30 bitrate=500 \
             option-string=repeat-headers=1:aud=1:open-gop=0 \
             ! h265parse config-interval=-1 ! rtph265pay name=pay0 pt=96 config-interval=1"
        }
        TestCodec::Vp8 => {
            "vp8enc deadline=1 cpu-used=8 keyframe-max-dist=30 lag-in-frames=0 \
             target-bitrate=500000 ! rtpvp8pay name=pay0 pt=96"
        }
        TestCodec::Vp9 => {
            "vp9enc deadline=1 cpu-used=8 keyframe-max-dist=30 lag-in-frames=0 \
             target-bitrate=500000 ! rtpvp9pay name=pay0 pt=96"
        }
        TestCodec::Av1 => {
            "av1enc cpu-used=8 usage-profile=realtime keyframe-max-dist=30 lag-in-frames=0 \
             target-bitrate=500 ! av1parse \
             ! video/x-av1,stream-format=obu-stream,alignment=tu ! rtpav1pay name=pay0 pt=96"
        }
    };
    format!(
        "videotestsrc is-live=true \
         ! video/x-raw,width={},height={},framerate=30/1 ! videoconvert ! {encode}",
        resolution.width, resolution.height,
    )
}

/// Builds a pipeline at the default resolution.
pub fn default_pipeline(codec: TestCodec) -> String {
    pipeline(codec, TEST_RESOLUTION)
}

/// Builds a two-track pipeline: video at the default resolution plus a PCMA
/// audio track, mirroring the SDP shape of a typical camera.
pub fn default_pipeline_with_audio(codec: TestCodec) -> String {
    format!(
        "{} audiotestsrc is-live=true ! alawenc ! rtppcmapay name=pay1 pt=8",
        default_pipeline(codec),
    )
}

/// An in-process GStreamer RTSP server serving one launch pipeline at
/// `/test` on an ephemeral localhost port.
pub struct RtspTestServer {
    main_loop: glib::MainLoop,
    thread: Option<thread::JoinHandle<()>>,
    port: i32,
    tls: bool,
}

/// Authentication required by a test server.
#[derive(Debug, Clone, Copy)]
enum TestAuth<'a> {
    None,
    Basic { username: &'a str, password: &'a str },
    Digest { username: &'a str, password: &'a str },
}

impl RtspTestServer {
    /// Starts a server streaming `media_pipeline`, which must contain an RTP
    /// payloader named `pay0`.
    pub fn launch(media_pipeline: &str) -> Self {
        Self::launch_inner(media_pipeline, TestAuth::None, false)
    }

    /// Starts a server that requires Basic authentication.
    pub fn launch_with_basic_auth(
        media_pipeline: &str,
        username: &str,
        password: &str,
    ) -> Self {
        Self::launch_inner(media_pipeline, TestAuth::Basic { username, password }, false)
    }

    /// Starts a server that requires Digest authentication.
    pub fn launch_with_digest_auth(
        media_pipeline: &str,
        username: &str,
        password: &str,
    ) -> Self {
        Self::launch_inner(media_pipeline, TestAuth::Digest { username, password }, false)
    }

    /// Starts a server that requires TLS (`rtsps://`), presenting a
    /// freshly generated self-signed certificate.
    pub fn launch_tls(media_pipeline: &str) -> Self {
        Self::launch_inner(media_pipeline, TestAuth::None, true)
    }

    /// Starts a server that requires both TLS and Digest authentication.
    pub fn launch_tls_with_digest_auth(
        media_pipeline: &str,
        username: &str,
        password: &str,
    ) -> Self {
        Self::launch_inner(media_pipeline, TestAuth::Digest { username, password }, true)
    }

    fn launch_inner(media_pipeline: &str, test_auth: TestAuth<'_>, tls: bool) -> Self {
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

        if !matches!(test_auth, TestAuth::None) || tls {
            factory.add_role_from_structure(
                &gst::Structure::builder("user")
                    .field("media.factory.access", true)
                    .field("media.factory.construct", true)
                    .build(),
            );
            let auth = RTSPAuth::new();
            if tls {
                // Once a certificate is set, gst-rtsp-server requires TLS on
                // every connection to this server instance.
                auth.set_tls_certificate(Some(&self_signed_certificate()));
            }
            let token = RTSPToken::builder().field("media.factory.role", "user").build();
            match test_auth {
                TestAuth::None => {
                    // TLS without authentication: admit anonymous clients.
                    let mut token = token;
                    auth.set_default_token(Some(&mut token));
                }
                TestAuth::Basic { username, password } => {
                    auth.set_supported_methods(gstreamer_rtsp::RTSPAuthMethod::Basic);
                    auth.add_basic(&RTSPAuth::make_basic(username, password), &token);
                }
                TestAuth::Digest { username, password } => {
                    auth.set_supported_methods(gstreamer_rtsp::RTSPAuthMethod::Digest);
                    auth.add_digest(username, password, &token);
                }
            }
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
            "RTSP test server listening at {}://127.0.0.1:{port}/test{}",
            if tls { "rtsps" } else { "rtsp" },
            match test_auth {
                TestAuth::None => "",
                TestAuth::Basic { .. } => " (basic auth)",
                TestAuth::Digest { .. } => " (digest auth)",
            },
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

        Self { main_loop, thread: Some(thread), port, tls }
    }

    /// The stream's RTSP or RTSPS URL.
    pub fn url(&self) -> String {
        let scheme = if self.tls { "rtsps" } else { "rtsp" };
        format!("{scheme}://127.0.0.1:{}/test", self.port)
    }
}

/// Generates a fresh self-signed certificate for the test server.
fn self_signed_certificate() -> gio::TlsCertificate {
    let certified = rcgen::generate_simple_self_signed(vec![
        "localhost".to_owned(),
        "127.0.0.1".to_owned(),
    ])
    .expect("failed to generate a self-signed certificate");
    let pem = format!("{}{}", certified.cert.pem(), certified.key_pair.serialize_pem());
    gio::TlsCertificate::from_pem(&pem).expect("failed to load the certificate into GIO")
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
        accept_invalid_tls_certs: false,
    }
}
