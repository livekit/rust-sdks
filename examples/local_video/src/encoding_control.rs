use serde::{Deserialize, Serialize};

/// RPC method name used to negotiate video encoding limits between the
/// subscriber (caller) and the publisher (responder).
pub const SET_VIDEO_ENCODING_LIMITS_METHOD: &str = "set-video-encoding-limits";

/// Payload sent by the subscriber to request new video encoding limits on a
/// publisher's track.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SetEncodingLimitsRequest {
    pub track_sid: String,
    pub bitrate_bps: Option<u64>,
    pub max_framerate: Option<f64>,
    pub scale_resolution_down_by: Option<f64>,
    pub reason: String,
}

/// Payload returned by the publisher describing the encoding limits it applied.
#[derive(Debug, Deserialize, Serialize)]
pub struct SetEncodingLimitsResponse {
    pub applied_bitrate_bps: Option<u64>,
    pub applied_max_framerate: Option<f64>,
    pub applied_scale_resolution_down_by: Option<f64>,
    pub track_sid: String,
}
