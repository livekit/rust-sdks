# Ignore this file if you have no intention of using a custom libwebrtc build.

import platform
import subprocess

# Edit the target_os/target_cpu to match your platform.
GN_ARGS = [
    "is_debug=true",
    "treat_warnings_as_errors=false",
    'target_os="mac"',
    'target_cpu="arm64"',
    "rtc_include_tests=false",
    "rtc_use_h264=false",
    "is_component_build=false",
    "rtc_build_examples=false",
    "use_rtti=true",
    "rtc_build_tools=false",
    "use_custom_libcxx=false",
]

cmd = ["gn", "gen", "out/Dev", "--args=" + ' '.join(GN_ARGS)]
print("Executing:", cmd)
subprocess.call(cmd, shell=platform.system() == "Windows")

# Use this command when developing on libwebrtc:  
# ninja -C out/Dev sdk:default_codec_factory_objc api api/task_queue:default_task_queue_factory api/audio_codecs:builtin_audio_decoder_factory sdk:videocapture_objc pc:peerconnection sdk:native_api callback_logger_objc default
