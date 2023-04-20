@echo off

if not exist depot_tools (
  git clone --depth 1 https://chromium.googlesource.com/chromium/tools/depot_tools.git
)

set COMMAND_DIR=%~dp0
set PATH=%cd%\depot_tools;%PATH%
set DEPOT_TOOLS_WIN_TOOLCHAIN=0
set GYP_GENERATORS=ninja,msvs-ninja
set GYP_MSVS_VERSION=2019
set OUTPUT_DIR=src/out
set ARTIFACTS_DIR=%cd%\windows
set vs2019_install=C:\Program Files (x86)\Microsoft Visual Studio\2019\Professional

if not exist src (
  call gclient.bat sync -D --no-history
)

cd src
call git apply "%COMMAND_DIR%/patches/add_license_dav1d.patch" -v
call git apply "%COMMAND_DIR%/patches/ssl_verify_callback_with_native_handle.patch" -v
call git apply "%COMMAND_DIR%/patches/fix_mocks.patch" -v
cd ..

mkdir "%ARTIFACTS_DIR%\lib"

setlocal enabledelayedexpansion

for %%i in (x64 arm64) do (
  mkdir "%ARTIFACTS_DIR%/lib/%%i"
  for %%j in (true false) do (

    rem generate ninja for release
    call gn.bat gen %OUTPUT_DIR% --root="src" ^
      --args="is_debug=%%j is_clang=true target_cpu=\"%%i\" use_custom_libcxx=false rtc_include_tests=false rtc_build_examples=false rtc_use_h264=false symbol_level=0 enable_iterator_debugging=false"

    rem build
    ninja.exe -C %OUTPUT_DIR% webrtc

    set filename=
    if true==%%j (
      set filename=webrtcd.lib
    ) else (
      set filename=webrtc.lib
    )

    rem copy static library for release build
    copy "%OUTPUT_DIR%\obj\webrtc.lib" "%ARTIFACTS_DIR%\lib\%%i\!filename!"
  )
)

endlocal

rem generate license
call python3 "%cd%\src\tools_webrtc\libs\generate_licenses.py" ^
  --target :webrtc %OUTPUT_DIR% %OUTPUT_DIR%

rem copy header
xcopy src\*.h "%ARTIFACTS_DIR%\include" /C /S /I /F /H

rem copy license
copy "%OUTPUT_DIR%\LICENSE.md" "%ARTIFACTS_DIR%\LICENSE.md"
