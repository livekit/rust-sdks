FROM nvidia/cuda:12.9.1-base-ubuntu24.04

ARG DEBIAN_FRONTEND=noninteractive
ARG LLVM_VERSION=21.1.8
ARG PROTOC_VERSION=25.2

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        cmake \
        cuda-cudart-dev-12-9 \
        cuda-driver-dev-12-9 \
        curl \
        git \
        libasound2-dev \
        libdrm-dev \
        libgbm-dev \
        libgl1-mesa-dev \
        libglib2.0-dev \
        libssl-dev \
        libva-dev \
        libx11-dev \
        libxcomposite-dev \
        libxdamage-dev \
        libxext-dev \
        libxfixes-dev \
        libxrandr-dev \
        ninja-build \
        pkg-config \
        python3 \
        unzip \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

RUN curl --fail --location --silent --show-error \
        "https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-linux-x86_64.zip" \
        --output /tmp/protoc.zip \
    && unzip -q /tmp/protoc.zip -d /usr/local \
    && rm /tmp/protoc.zip \
    && protoc --version

COPY .github/scripts/install-clang.sh /tmp/install-clang.sh
RUN LLVM_VERSION="${LLVM_VERSION}" LLVM_ROOT="/opt/llvm-${LLVM_VERSION}" \
        /tmp/install-clang.sh \
    && rm /tmp/install-clang.sh

ENV LLVM_ROOT=/opt/llvm-21.1.8
ENV CC=/opt/llvm-21.1.8/bin/clang
ENV CXX=/opt/llvm-21.1.8/bin/clang++
ENV CUDA_HOME=/usr/local/cuda
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH=/usr/local/cargo/bin:/opt/llvm-21.1.8/bin:${PATH}

RUN curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error \
        https://sh.rustup.rs \
        | sh -s -- -y --profile default --default-toolchain 1.97.1 \
    && rustc --version \
    && cargo --version

WORKDIR /workspace
