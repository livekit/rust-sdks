# Copyright 2025 LiveKit, Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# This file is used to download prebuilt binaries of livekit-ffi from our GH releases.
# It is mostly used by our bindings (e.g https://github.com/livekit/client-sdk-python)
# By default, the script will try to autodetect the platform, useful to simplify the CI.


import argparse
import os
import platform
import re
import sys
import tempfile
from zipfile import ZipFile

import requests


def target_os():
    if sys.platform.startswith("win"):
        return "windows"
    elif sys.platform.startswith("darwin"):
        return "macos"
    elif sys.platform.startswith("linux"):
        return "linux"

    return None


def target_arch():
    arch = platform.machine().lower()
    arch_mapping = {
        "amd64": "x86_64",
        "x86_64": "x86_64",
        "arm64": "arm64",
        "aarch64": "arm64",
        "armv7": "armv7",
        "armv7l": "armv7",
    }

    # make sure cibuildwheel download the right binary when cross-compiling to arm64
    # TODO: Make this more generic
    print("initial arch: %s" % arch)
    if os.environ.get("CIBUILDWHEEL") and target_os() == "macos":
        archflags = os.environ.get("ARCHFLAGS", "")
        print("cibuildwheel is being used, archflags: %s" % archflags)
        if "arm64" in archflags:
            return "arm64"
        else:
            return "x86_64"

    return arch_mapping.get(arch)


# extract the version from livekit-ffi/Cargo.toml
def ffi_version():
    file = os.path.join(os.path.dirname(__file__), "livekit-ffi", "Cargo.toml")
    with open(file, "r") as f:
        return re.search(r'version\s*=\s*"(\d+\.\d+\.\d+)"', f.read()).group(1)


def download_ffi(platform, arch, version, output):
    filename = "ffi-%s-%s.zip" % (platform, arch)
    url = "https://github.com/livekit/client-sdk-rust/releases/download/rust-sdks/livekit-ffi@%s/%s"
    url = url % (version, filename)

    tmp = os.path.join(tempfile.gettempdir(), filename)

    # download the binary
    resp = requests.get(url, stream=True)
    if resp.status_code != 200:
        raise Exception("failed to download, status: %d" % (resp.status_code))

    with open(tmp, mode="wb") as f:
        for chunk in resp.iter_content(chunk_size=1024 * 8):
            f.write(chunk)

    # unzip to output
    zip = ZipFile(tmp)
    os.makedirs(output, exist_ok=True)
    zip.extractall(output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--platform",
        help="target platform",
        default=target_os(),
        choices=["macos", "linux", "windows", "ios", "android"],
    )
    parser.add_argument(
        "--arch",
        help="target architecture",
        default=target_arch(),
        choices=["x86_64", "arm64", "armv7"],
    )
    parser.add_argument(
        "--version",
        help="version to download",
        default=ffi_version(),
    )
    parser.add_argument("--output", help="output path", required=True)
    args = parser.parse_args()

    print(
        "downloading livekit-ffi v%s for %s-%s"
        % (args.version, args.platform, args.arch)
    )
    download_ffi(args.platform, args.arch, args.version, args.output)
    print("downloaded to %s" % os.path.abspath(args.output))
