# Copyright 2026 LiveKit, Inc.
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

ROOT_DIR := $(shell cd .. && pwd)

X86_IMAGE    := livekit-rust-env:x86_64
AARCH64_IMAGE := livekit-rust-env:aarch64

X86_TARGET    := x86_64-unknown-linux-gnu
AARCH64_TARGET := aarch64-unknown-linux-gnu

DOCKER_RUN = docker run --rm -v $(ROOT_DIR):/workspace -w /workspace

.PHONY: submodules env-x86_64 env-aarch64 sdk-x86_64 sdk-aarch64 ffi-x86_64 ffi-aarch64 clean

# --- Submodules ---

submodules:
	git -C $(ROOT_DIR) submodule update --init --recursive

# --- Environment images ---

env-x86_64:
	docker build -f Dockerfile.linux \
		-t $(X86_IMAGE) .

env-aarch64:
	docker build -f Dockerfile.linux \
		--build-arg BASE_IMAGE=quay.io/pypa/manylinux_2_28_aarch64 \
		--build-arg RUST_TARGET=$(AARCH64_TARGET) \
		-t $(AARCH64_IMAGE) .

# --- SDK builds ---

sdk-x86_64: submodules env-x86_64
	$(DOCKER_RUN) $(X86_IMAGE) \
		cargo build --release --target $(X86_TARGET) -p livekit

sdk-aarch64: submodules env-aarch64
	$(DOCKER_RUN) $(AARCH64_IMAGE) \
		cargo build --release --target $(AARCH64_TARGET) -p livekit

# --- FFI builds ---

ffi-x86_64: submodules env-x86_64
	$(DOCKER_RUN) $(X86_IMAGE) \
		bash -c "cd livekit-ffi && cargo build --release --target $(X86_TARGET)"

ffi-aarch64: submodules env-aarch64
	$(DOCKER_RUN) $(AARCH64_IMAGE) \
		bash -c "cd livekit-ffi && cargo build --release --target $(AARCH64_TARGET)"

# --- Cleanup ---

clean:
	-docker rmi $(X86_IMAGE) $(AARCH64_IMAGE) 2>/dev/null
