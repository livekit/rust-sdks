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


# The stubs generated below carry Implib.so's MIT license header. Keep
# ./LICENSE.txt and the "Implib.so MIT License" section of webrtc-sys/NOTICE.md
# in sync with the upstream LICENSE.txt when bumping Implib.so.
if [ ! -e "$(pwd)/Implib.so" ]
then
  git clone --depth 1 https://github.com/yugr/Implib.so.git
fi

generate_implib() {
   category=$1
   libname=$2
   arch=$3
   echo "Generating implib for category: ${category} libname: ${libname} - ${arch}, output to ${category}/${arch}/"
   mkdir -p ${category}/${arch}/
   python3 $(pwd)/Implib.so/implib-gen.py /lib/x86_64-linux-gnu/${libname}.so --target ${arch} --outdir ${category}/${arch}/
}

desktop_capturer_deps=("libdrm" "libgbm" "libXfixes" "libXdamage" "libXcomposite" "libXrandr" "libXext" "libX11")

for dep in "${desktop_capturer_deps[@]}"
do
  generate_implib "desktop_capturer" ${dep} "x86_64-linux-gnu"
  generate_implib "desktop_capturer" ${dep} "aarch64-linux-gnu"
done

nvidia_deps=("libcuda" "libnvcuvid")

for dep in "${nvidia_deps[@]}"
do
  generate_implib "nvidia" ${dep} "x86_64-linux-gnu"
  generate_implib "nvidia" ${dep} "aarch64-linux-gnu"
done


vaapi_deps=("libva" "libva-drm")
for dep in "${vaapi_deps[@]}"
do
  generate_implib "vaapi" ${dep} "x86_64-linux-gnu"
  generate_implib "vaapi" ${dep} "aarch64-linux-gnu"
done

# Jetson (L4T) userspace libraries (aarch64 only). Unlike the libraries
# above, these are not installable on a regular build host, so they are
# pulled from NVIDIA's L4T apt repository. The generated stubs load through
# the lk_jetson_dlopen callback (src/jetson/jetson_runtime_loader.cpp)
# instead of a plain dlopen, so the L4T vendor directories
# (/usr/lib/aarch64-linux-gnu/{nvidia,tegra}) are searched even when they
# are not in the loader's default path. libnvv4l2.so is L4T's build of
# libv4l2 (its SONAME is libv4l2.so.0); the input files are renamed so the
# output filenames match what build.rs expects from add_lazy_load_so.
generate_jetson_implibs() {
  local l4t_repo="https://repo.download.nvidia.com/jetson"
  local l4t_version="36.4.7-20250918154033"
  local workdir
  workdir=$(mktemp -d)

  curl -fsSL -o "${workdir}/multimedia.deb" \
    "${l4t_repo}/t234/pool/main/n/nvidia-l4t-multimedia/nvidia-l4t-multimedia_${l4t_version}_arm64.deb"
  curl -fsSL -o "${workdir}/multimedia-utils.deb" \
    "${l4t_repo}/t234/pool/main/n/nvidia-l4t-multimedia-utils/nvidia-l4t-multimedia-utils_${l4t_version}_arm64.deb"
  for deb in multimedia multimedia-utils; do
    mkdir -p "${workdir}/${deb}"
    (cd "${workdir}/${deb}" && ar x "../${deb}.deb" && tar -xf data.tar.*)
  done

  local nvidia_lib_dir_rel="usr/lib/aarch64-linux-gnu/nvidia"
  cp "${workdir}/multimedia/${nvidia_lib_dir_rel}/libnvv4l2.so" "${workdir}/libv4l2.so"
  cp "${workdir}/multimedia-utils/${nvidia_lib_dir_rel}/libnvbufsurface.so.1.0.0" \
    "${workdir}/libnvbufsurface.so"

  mkdir -p jetson/aarch64-linux-gnu
  for lib in libv4l2 libnvbufsurface; do
    python3 "$(pwd)/Implib.so/implib-gen.py" "${workdir}/${lib}.so" \
      --target aarch64 \
      --dlopen-callback lk_jetson_dlopen \
      --outdir jetson/aarch64-linux-gnu/
  done

  rm -rf "${workdir}"
}

generate_jetson_implibs
