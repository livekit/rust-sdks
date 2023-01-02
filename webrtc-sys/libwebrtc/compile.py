import subprocess

GN_ARGS = [
    "is_debug=false",
    "treat_warnings_as_errors=false",
    'target_os="win"',
    'target_cpu="x64"',
    "rtc_include_tests=false",
    "rtc_use_h264=false",
    "is_component_build=false",
    "rtc_build_examples=false",
    "use_rtti=true",
    "rtc_build_tools=false",
    "use_custom_libcxx=false",
    "strip_debug_info=true",
    "symbol_level=0"
]

subprocess.call(["gn", "gen", "out/Default", "--args=" + ' '.join(GN_ARGS)], shell=True)
