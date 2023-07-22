FROM arm64v8/debian:latest

RUN apt-get update \
    && apt-get install -y curl \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    && . $HOME/.cargo/env \
    && rustup target add aarch64-unknown-linux-gnu

ENV PATH="/root/.cargo/bin:${PATH}"

RUN apt-get install -y build-essential pkg-config libssl-dev libx11-dev libgl1-mesa-dev libxext-dev

WORKDIR /usr/src/app

COPY . .