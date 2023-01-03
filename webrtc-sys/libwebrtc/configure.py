import platform
import subprocess

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
