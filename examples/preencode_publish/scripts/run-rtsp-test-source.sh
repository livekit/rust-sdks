#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=examples/preencode_publish/scripts/gst-test-source-common.sh
source "$SCRIPT_DIR/gst-test-source-common.sh"

PORT=8554
CODEC=

usage() {
    cat <<'USAGE'
Usage: run-rtsp-test-source.sh --codec h264|h265|vp8|vp9|av1 [options]

Starts a gst-rtsp-server test-launch server that serves an animated
test-pattern stream at rtsp://127.0.0.1:PORT/test.

Options:
  --codec CODEC            Required encoded codec.
  --port PORT              RTSP server port. Default: 8554.
  --width PIXELS           Source width. Default: 1280.
  --height PIXELS          Source height. Default: 720.
  --fps FPS                Source frame rate. Default: 30.
  --bitrate-kbps KBPS      Encoder bitrate. Default: 2500.
  --print                  Print the test-launch command instead of running it.
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

PIPELINE="( $(gst_animated_video_source) ! $(gst_encoded_access_unit_pipeline "$CODEC") ! $(gst_rtp_payloader_pipeline "$CODEC") )"

echo "Serving $CODEC RTSP test pattern at rtsp://127.0.0.1:$PORT/test" >&2
gst_run_rtsp_launch_line "$PORT" "$PIPELINE"
