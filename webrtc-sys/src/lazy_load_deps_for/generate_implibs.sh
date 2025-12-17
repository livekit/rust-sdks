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


if [ ! -e "$(pwd)/Implib.so" ]
then
  git clone --depth 1 https://github.com/yugr/Implib.so.git
fi

mkdir -p desktop_capturer/x86_64-linux-gnu/
mkdir -p desktop_capturer/aarch64-linux-gnu/

desktop_capturer_deps=("libdrm" "libgbm" "libXfixes" "libXdamage" "libXcomposite" "libXrandr" "libXext" "libX11")

for dep in "${desktop_capturer_deps[@]}"
do
  python3 $(pwd)/Implib.so/implib-gen.py /lib/x86_64-linux-gnu/${dep}.so --target x86_64-linux-gnu --outdir desktop_capturer/x86_64-linux-gnu/
  python3 $(pwd)/Implib.so/implib-gen.py /lib/x86_64-linux-gnu/${dep}.so --target aarch64-linux-gnu --outdir desktop_capturer/aarch64-linux-gnu/
done

mkdir -p nvidia/x86_64-linux-gnu/
mkdir -p nvidia/aarch64-linux-gnu/

nvidia_deps=("libcuda" "libnvcuvid")

for dep in "${nvidia_deps[@]}"
do
  python3 $(pwd)/Implib.so/implib-gen.py /usr/lib/x86_64-linux-gnu/${dep}.so --target x86_64-linux-gnu --outdir nvidia/x86_64-linux-gnu/
  python3 $(pwd)/Implib.so/implib-gen.py /usr/lib/x86_64-linux-gnu/${dep}.so --target aarch64-linux-gnu --outdir nvidia/aarch64-linux-gnu/
done    

mkdir -p vaapi/x86_64-linux-gnu/
mkdir -p vaapi/aarch64-linux-gnu/

vaapi_deps=("libva" "libva-drm")
for dep in "${vaapi_deps[@]}"
do
    python3 $(pwd)/Implib.so/implib-gen.py /usr/lib/x86_64-linux-gnu/${dep}.so --target x86_64-linux-gnu --outdir vaapi/x86_64-linux-gnu/
    python3 $(pwd)/Implib.so/implib-gen.py /usr/lib/x86_64-linux-gnu/${dep}.so --target aarch64-linux-gnu --outdir vaapi/aarch64-linux-gnu/
done

