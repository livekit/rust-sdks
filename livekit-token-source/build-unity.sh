#!/usr/bin/env bash
#
# Build the livekit-token-source Rust cdylib (release, arm64), deploy it plus
# freshly generated C# UniFFI bindings into the Unity project, and downgrade the
# bindings to be C# 9 compatible.
#
# Unity 2022.3 LTS is pinned to C# 9. uniffi-bindgen-cs emits one C# 10 feature
# (a file-scoped `namespace X;`), which we rewrite to block-scoped `namespace X {…}`.
# Everything else it emits already falls in the C# 9 / DllImport branch under Mono.
#
# Note on `uniffi-bindgen-cs`: it always shells out to a full `cargo metadata`
# (with dependency resolution) in the current directory before generating. Run
# inside the big rust-sdks workspace that stalls — it tries to resolve/download
# every member's deps (crates.io index, uncached example deps, git deps), which
# hangs on a flaky/offline connection. All the type info the generator actually
# needs is baked into the dylib, so we run it from a throwaway single-crate
# workspace instead: `cargo metadata` there is instant and needs no network.

set -euo pipefail

# ---------------------------------------------------------------------------
# Config — adjust here if paths/names change.
# ---------------------------------------------------------------------------
# Cargo lib name. [lib] name is unset in Cargo.toml, so it defaults to the package
# name with dashes->underscores. Everything derives from this: liblivekit_token_source.dylib,
# DllImport("livekit_token_source"), namespace uniffi.livekit_token_source, livekit_token_source.cs.
LIB_NAME="livekit_token_source"
RUST_TARGET="aarch64-apple-darwin"       # matches the ffi-macos-arm64 plugin folder
DYLIB="lib${LIB_NAME}.dylib"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"   # livekit-token-source/ sits directly under the workspace root
UNITY_ROOT="$HOME/dev/unity/client-sdk-unity"
PLUGIN_DIR="$UNITY_ROOT/Runtime/Plugins/ffi-macos-arm64"
BINDINGS_DIR="$UNITY_ROOT/Runtime/Scripts/UniFFI"

DYLIB_SRC="$WORKSPACE_ROOT/target/$RUST_TARGET/release/$DYLIB"
BINDINGS_FILE="$BINDINGS_DIR/${LIB_NAME}.cs"

cd "$WORKSPACE_ROOT"

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------
if ! command -v uniffi-bindgen-cs >/dev/null 2>&1; then
  echo "error: uniffi-bindgen-cs not found on PATH." >&2
  echo "Install the release matching uniffi 0.31.0:" >&2
  echo "  cargo install uniffi-bindgen-cs \\" >&2
  echo "    --git https://github.com/NordSecurity/uniffi-bindgen-cs --tag v0.11.0+v0.31.0" >&2
  exit 1
fi

if ! rustup target list --installed 2>/dev/null | grep -qx "$RUST_TARGET"; then
  echo "error: Rust target '$RUST_TARGET' is not installed." >&2
  echo "  rustup target add $RUST_TARGET" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# 1. Compile the dylib (release, arm64)
# ---------------------------------------------------------------------------
echo "==> Building $DYLIB for $RUST_TARGET (release)"
cargo build --release --target "$RUST_TARGET" -p livekit-token-source

if [[ ! -f "$DYLIB_SRC" ]]; then
  echo "error: expected dylib not found at $DYLIB_SRC" >&2
  exit 1
fi

# Sanity-check the architecture matches the target plugin folder.
if ! file "$DYLIB_SRC" | grep -q "arm64"; then
  echo "error: $DYLIB is not arm64 — refusing to copy into ffi-macos-arm64." >&2
  file "$DYLIB_SRC" >&2
  exit 1
fi

# UniFFI metadata symbols the generator reads survive macOS `strip = "symbols"`
# (unlike Linux), so a stripped release dylib is fine to generate from.
if ! nm -gU "$DYLIB_SRC" 2>/dev/null | grep -q "UNIFFI_META"; then
  echo "error: no UNIFFI_META symbols in $DYLIB — bindgen would find nothing." >&2
  echo "       If this ever happens, rebuild with CARGO_PROFILE_RELEASE_STRIP=false." >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# 2. Copy the dylib into the Unity plugins folder (overwrite)
# ---------------------------------------------------------------------------
echo "==> Deploying $DYLIB -> $PLUGIN_DIR"
mkdir -p "$PLUGIN_DIR"
cp -f "$DYLIB_SRC" "$PLUGIN_DIR/$DYLIB"

# ---------------------------------------------------------------------------
# 3. Generate C# bindings into the Unity scripts folder (overwrite)
#
#    Run from a throwaway one-crate workspace so uniffi-bindgen-cs's mandatory
#    `cargo metadata` resolves instantly and offline (see header note).
#    --no-format skips the external dotnet/csharpier formatter (not needed;
#    absent it hangs, and we do our own C# 9 cleanup below anyway).
# ---------------------------------------------------------------------------
echo "==> Generating C# bindings -> $BINDINGS_DIR"
mkdir -p "$BINDINGS_DIR"

GENDIR="$(mktemp -d)"
trap 'rm -rf "$GENDIR"' EXIT
cat > "$GENDIR/Cargo.toml" <<'EOF'
[package]
name = "tokensource-bindgen-shim"
version = "0.0.0"
edition = "2021"

[lib]
path = "lib.rs"

# Own workspace root so cargo never walks up into a real workspace.
[workspace]
EOF
touch "$GENDIR/lib.rs"

# Pass a uniffi.toml through if one is ever added next to the crate.
CONFIG_ARGS=()
if [[ -f "$SCRIPT_DIR/uniffi.toml" ]]; then
  CONFIG_ARGS=(--config "$SCRIPT_DIR/uniffi.toml")
fi

# Guarded expansion: safe when the array is empty under `set -u` on bash 3.2 (macOS default).
( cd "$GENDIR" && uniffi-bindgen-cs --no-format ${CONFIG_ARGS[@]+"${CONFIG_ARGS[@]}"} --library "$DYLIB_SRC" --out-dir "$BINDINGS_DIR" )

if [[ ! -f "$BINDINGS_FILE" ]]; then
  echo "error: expected bindings file not found at $BINDINGS_FILE" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# 4. Downgrade the generated bindings to C# 9 (Unity's language version, still
#    true through Unity 6). uniffi-bindgen-cs emits a couple of C# 10 features;
#    csharp9-downgrade.pl rewrites them generically. Idempotent.
# ---------------------------------------------------------------------------
echo "==> Downgrading bindings to C# 9"
perl "$SCRIPT_DIR/csharp9-downgrade.pl" "$BINDINGS_DIR"/*.cs

echo "==> Done."
echo "    plugin:   $PLUGIN_DIR/$DYLIB"
echo "    bindings: $BINDINGS_FILE"
