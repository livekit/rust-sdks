#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=examples/preencode_publish/scripts/gst-test-source-common.sh
source "$SCRIPT_DIR/gst-test-source-common.sh"

SOCKET_PATH=/tmp/livekit-preencode-test.shm
SHM_SIZE=67108864
CODEC=

usage() {
    cat <<'USAGE'
Usage: run-shm-test-source.sh --codec h264|h265|vp8|vp9|av1 [options]

Starts a GStreamer animated test-pattern encoder that writes encoded access
units to shmsink.

Options:
  --codec CODEC            Required encoded codec.
  --socket-path PATH       shmsink control socket. Default: /tmp/livekit-preencode-test.shm.
  --shm-size BYTES         Shared-memory buffer size. Default: 67108864.
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
        --socket-path)
            [ "$#" -ge 2 ] || gst_error "--socket-path requires a value"
            SOCKET_PATH=$2
            shift 2
            ;;
        --shm-size)
            [ "$#" -ge 2 ] || gst_error "--shm-size requires a value"
            SHM_SIZE=$2
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
gst_validate_positive_int "--shm-size" "$SHM_SIZE"

if [ "$PRINT_ONLY" -eq 0 ]; then
    rm -f "$SOCKET_PATH"
fi
PIPELINE="$(gst_animated_video_source) ! $(gst_encoded_access_unit_pipeline "$CODEC") ! shmsink socket-path=$SOCKET_PATH shm-size=$SHM_SIZE wait-for-connection=true sync=true"

echo "Writing $CODEC test pattern to shmsink socket $SOCKET_PATH" >&2
gst_run_launch_line "$PIPELINE"
