#!/bin/bash
# Copyright 2023 LiveKit, Inc.
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

arch=""
platform=""
version=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --arch)
      arch="$2"
      if [ "$arch" != "x86_64" ] && [ "$arch" != "arm" ] && [ "$arch" != "arm64" ]; then
        echo "Error: Invalid value for --arch. Must be 'x86_64', 'arm' or 'arm64'."
        exit 1
      fi
      shift 2
      ;;
    --platform)
      platform="$2"
      if [ "$platform" != "windows" ] \
            && [ "$platform" != "linux" ] \
            && [ "$platform" != "macos" ] \
            && [ "$platform" != "ios" ] \
            && [ "$platform" != "android" ]; then

        echo "Error: Invalid value for --platform. Must be 'windows', 'linux', 'macos', 'ios' or 'android'."
        exit 1
      fi
      shift 2
      ;;
    --version)
      version="$2"
      shift 2
      ;;
    *)
      echo "Error: Unknown argument '$1'"
      exit 1
      ;;
  esac
done

if [ -z "$arch" ]; then
  echo "Error: --arch must be set."
  exit 1
fi

if [ -z "$platform" ]; then
  echo "Error: --platform must be set."
  exit 1
fi

if [ -z "$version" ]; then
  echo "Error: --version must be set."
  exit 1
fi

url="https://github.com/livekit/client-sdk-rust/releases/download/ffi-v$version/ffi-$platform-$arch.zip"
echo "Downloading $url"
curl $url -L --fail --output ffi-$platform-$arch.zip