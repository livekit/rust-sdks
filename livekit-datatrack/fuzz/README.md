# Packet deserialization fuzzing

Mutation-based fuzzing for `livekit-datatrack`, built with [`cargo-fuzz`](https://rust-fuzz.github.io/book/cargo-fuzz.html).

## Prerequisites

```sh
rustup install nightly
cargo install cargo-fuzz
```

## Basic usage

From this directory:

```sh
cargo +nightly fuzz run packet_deserialize -- -max_len=256
```

See `cargo +nightly fuzz --help` for more options.
