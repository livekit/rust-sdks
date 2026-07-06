#!/usr/bin/env bash

set -euo pipefail

GST_LAUNCH=${GST_LAUNCH:-gst-launch-1.0}
GST_RTSP_TEST_LAUNCH=${GST_RTSP_TEST_LAUNCH:-test-launch}

WIDTH=1280
HEIGHT=720
FPS=30
BITRATE_KBPS=2500
PRINT_ONLY=0
GST_COMMON_SHIFT=0

gst_error() {
    echo "error: $*" >&2
    exit 1
}

gst_require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        gst_error "required command not found: $1"
    fi
}

gst_validate_positive_int() {
    local name=$1
    local value=$2

    case "$value" in
        ''|*[!0-9]*)
            gst_error "$name must be a positive integer, got '$value'"
            ;;
    esac

    if [ "$value" -eq 0 ]; then
        gst_error "$name must be greater than zero"
    fi
}

gst_normalize_codec() {
    local codec
    codec=$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')

    case "$codec" in
        h264|h265|vp8|vp9|av1)
            printf '%s\n' "$codec"
            ;;
        *)
            return 1
            ;;
    esac
}

gst_parse_common_option() {
    GST_COMMON_SHIFT=0

    case "$1" in
        --width)
            [ "$#" -ge 2 ] || gst_error "--width requires a value"
            WIDTH=$2
            GST_COMMON_SHIFT=2
            ;;
        --height)
            [ "$#" -ge 2 ] || gst_error "--height requires a value"
            HEIGHT=$2
            GST_COMMON_SHIFT=2
            ;;
        --fps)
            [ "$#" -ge 2 ] || gst_error "--fps requires a value"
            FPS=$2
            GST_COMMON_SHIFT=2
            ;;
        --bitrate-kbps)
            [ "$#" -ge 2 ] || gst_error "--bitrate-kbps requires a value"
            BITRATE_KBPS=$2
            GST_COMMON_SHIFT=2
            ;;
        --print)
            PRINT_ONLY=1
            GST_COMMON_SHIFT=1
            ;;
        *)
            gst_error "unknown common option: $1"
            ;;
    esac
}

gst_validate_common_options() {
    gst_validate_positive_int "--width" "$WIDTH"
    gst_validate_positive_int "--height" "$HEIGHT"
    gst_validate_positive_int "--fps" "$FPS"
    gst_validate_positive_int "--bitrate-kbps" "$BITRATE_KBPS"
}

gst_animated_video_source() {
    printf 'videotestsrc is-live=true do-timestamp=true pattern=ball motion=wavy animation-mode=frames ! video/x-raw,width=%s,height=%s,framerate=%s/1 ! timeoverlay halignment=right valignment=bottom shaded-background=true ! videoconvert ! video/x-raw,format=I420 ! queue' \
        "$WIDTH" "$HEIGHT" "$FPS"
}

gst_encoded_access_unit_pipeline() {
    local codec=$1
    local key_int_max=$FPS

    case "$codec" in
        h264|h265)
            gst_h26x_annex_b_pipeline "$codec"
            ;;
        vp8)
            printf 'vp8enc deadline=1 cpu-used=8 keyframe-max-dist=%s lag-in-frames=0 target-bitrate=%s000 ! video/x-vp8' \
                "$key_int_max" "$BITRATE_KBPS"
            ;;
        vp9)
            printf 'vp9enc deadline=1 cpu-used=8 keyframe-max-dist=%s lag-in-frames=0 target-bitrate=%s000 ! video/x-vp9,profile=(string)0' \
                "$key_int_max" "$BITRATE_KBPS"
            ;;
        av1)
            printf 'av1enc cpu-used=8 usage-profile=realtime keyframe-max-dist=%s lag-in-frames=0 target-bitrate=%s ! av1parse ! video/x-av1,stream-format=obu-stream,alignment=tu' \
                "$key_int_max" "$BITRATE_KBPS"
            ;;
        *)
            gst_error "unsupported codec: $codec"
            ;;
    esac
}

gst_h26x_annex_b_pipeline() {
    local codec=$1
    local key_int_max=$FPS

    case "$codec" in
        h264)
            printf 'x264enc tune=zerolatency speed-preset=ultrafast key-int-max=%s bitrate=%s byte-stream=true aud=true ! h264parse config-interval=-1 ! video/x-h264,stream-format=byte-stream,alignment=au' \
                "$key_int_max" "$BITRATE_KBPS"
            ;;
        h265)
            printf 'x265enc tune=zerolatency speed-preset=ultrafast key-int-max=%s bitrate=%s option-string=repeat-headers=1:aud=1:open-gop=0 ! h265parse config-interval=-1 ! video/x-h265,stream-format=byte-stream,alignment=au' \
                "$key_int_max" "$BITRATE_KBPS"
            ;;
        *)
            gst_error "unsupported codec: $codec"
            ;;
    esac
}

gst_rtp_payloader_pipeline() {
    case "$1" in
        h264)
            printf 'rtph264pay name=pay0 pt=96 config-interval=1'
            ;;
        h265)
            printf 'rtph265pay name=pay0 pt=96 config-interval=1'
            ;;
        vp8)
            printf 'rtpvp8pay name=pay0 pt=96'
            ;;
        vp9)
            printf 'rtpvp9pay name=pay0 pt=96'
            ;;
        av1)
            printf 'rtpav1pay name=pay0 pt=96'
            ;;
        *)
            gst_error "unsupported codec: $1"
            ;;
    esac
}

gst_run_launch_line() {
    local pipeline=$1

    if [ "$PRINT_ONLY" -eq 1 ]; then
        printf 'pipeline=%q\n%q -e $pipeline\n' "$pipeline" "$GST_LAUNCH"
        return
    fi

    gst_require_command "$GST_LAUNCH"
    # Intentionally split the launch line into gst-launch arguments.
    # The line is assembled from validated flags and fixed pipeline fragments.
    exec "$GST_LAUNCH" -e $pipeline
}

gst_run_rtsp_launch_line() {
    local port=$1
    local pipeline=$2

    if [ "$PRINT_ONLY" -eq 1 ]; then
        printf '%q -p %q %q\n' "$GST_RTSP_TEST_LAUNCH" "$port" "$pipeline"
        return
    fi

    gst_require_command "$GST_RTSP_TEST_LAUNCH"
    exec "$GST_RTSP_TEST_LAUNCH" -p "$port" "$pipeline"
}
