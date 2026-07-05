#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=examples/preencode_publish/scripts/gst-test-source-common.sh
source "$SCRIPT_DIR/gst-test-source-common.sh"

HOST=127.0.0.1
PORT=5000
CODEC=

usage() {
    cat <<'USAGE'
Usage: run-tcp-test-source.sh --codec h264|h265|vp8|vp9|av1 [options]

Starts a GStreamer animated test-pattern encoder from tcpserversink.
H.264/H.265 are served as Annex-B byte streams. VP8/VP9/AV1 are served as
RFC4571-style length-prefixed RTP packets.

Options:
  --codec CODEC            Required encoded codec.
  --host HOST              Address to listen on. Default: 127.0.0.1.
  --port PORT              TCP port to listen on. Default: 5000.
  --width PIXELS           Source width. Default: 1280.
  --height PIXELS          Source height. Default: 720.
  --fps FPS                Source frame rate. Default: 30.
  --bitrate-kbps KBPS      Encoder bitrate. Default: 2500.
  --print                  Print the gst-launch command instead of running it.
  -h, --help               Show this help.
USAGE
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --codec)
            [ "$#" -ge 2 ] || gst_error "--codec requires h264, h265, vp8, vp9, or av1"
            CODEC=$2
            shift 2
            ;;
        --host)
            [ "$#" -ge 2 ] || gst_error "--host requires a value"
            HOST=$2
            shift 2
            ;;
        --port)
            [ "$#" -ge 2 ] || gst_error "--port requires a value"
            PORT=$2
            shift 2
            ;;
        --width|--height|--fps|--bitrate-kbps|--print)
            gst_parse_common_option "$@"
            shift "$GST_COMMON_SHIFT"
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            gst_error "unknown option: $1"
            ;;
    esac
done

[ -n "$CODEC" ] || gst_error "--codec is required"
if ! CODEC=$(gst_normalize_codec "$CODEC"); then
    gst_error "--codec must be h264, h265, vp8, vp9, or av1"
fi

gst_validate_common_options
gst_validate_positive_int "--port" "$PORT"

case "$CODEC" in
    h264|h265)
        PIPELINE="$(gst_animated_video_source) ! $(gst_h26x_annex_b_pipeline "$CODEC") ! tcpserversink host=$HOST port=$PORT sync-method=next-keyframe recover-policy=keyframe"
        FORMAT="Annex-B"
        ;;
    vp8|vp9|av1)
        PIPELINE="$(gst_animated_video_source) ! $(gst_encoded_access_unit_pipeline "$CODEC") ! $(gst_rtp_payloader_pipeline "$CODEC") ! rtpstreampay ! tcpserversink host=$HOST port=$PORT"
        FORMAT="RTP"
        ;;
    *)
        gst_error "unsupported codec: $CODEC"
        ;;
esac

echo "Serving $CODEC $FORMAT test pattern on tcp://$HOST:$PORT" >&2
gst_run_launch_line "$PIPELINE"
