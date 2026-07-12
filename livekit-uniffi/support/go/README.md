# LiveKit UniFFI — Go module

Support files for assembling a consumable Go module from the
[uniffi-bindgen-go](https://github.com/NordSecurity/uniffi-bindgen-go)
bindings. The bindgen is versioned in lockstep with uniffi-rs (the `+vX.Y.Z`
tag suffix); the tag pinned in `Makefile.toml` must target the same uniffi-rs
release as the workspace `uniffi` dependency.

## Prerequisites

- Go ≥ 1.19 on `PATH` (the bindgen runs `go fmt` on its output; cgo builds
  need a C toolchain)

## Full package build

From the crate root:

```bash
cargo make go-package                        # debug native lib
cargo make --profile release go-package      # release native lib
```

Output module at `packages/go`:

```
packages/go/
  go.mod                    (from go.mod.tera; module path = GO_MODULE)
  livekit_uniffi/           (generated bindings + C header per uniffi component)
  livekit_datatrack/
  livekit_uniffi/link.go    (from link.go here; cgo LDFLAGS)
  livekit_uniffi/*_test.go  (from test/)
  liblivekit_uniffi.<ext>   (dev: locally built dynamic library)
```

Consumers import the `livekit_uniffi` package; `livekit_datatrack` is
imported directly only for encryption providers and typed data-track errors:

```go
import uniffi "github.com/livekit/livekit-uniffi-go/livekit_uniffi"
```

## Linking

The generated bindings carry no cgo linker flags; `link.go` supplies
`-L${SRCDIR}/.. -llivekit_uniffi`, resolving the dynamic library from the
module root (where `go-copy-lib` placed the local build). On Linux an rpath
entry makes it resolve at runtime too; on macOS the dev dylib's install name
is the absolute path into `target/`, so it resolves as long as the local
Rust build exists. Windows consumers must arrange an import library and
`PATH` themselves. To link a library from a different location, set
`CGO_LDFLAGS` instead.

## Build notes

Context for the non-obvious parts of the `Makefile.toml` Go tasks:

- `bindgen-go` and `go-package` set `CARGO_PROFILE_RELEASE_STRIP=false`
  because library-mode bindgen reads `UNIFFI_META_*` symbols from the
  compiled cdylib, and the release profile's `strip = "symbols"` removes
  them on Linux (same workaround as `bindgen-dart`).
- The pinned bindgen tag's `+vX.Y.Z` suffix is the uniffi-rs release it
  targets and must match the workspace `uniffi` dependency, or the bindgen
  cannot read this crate's compiled metadata and the generated
  contract-version/checksum guards would panic at runtime.
- The `cargo install` args end with an explicit `uniffi-bindgen-go` package
  name: the bindgen repo is a workspace with several binary packages
  (fixtures), and cargo-make does not pass `crate_name` itself when
  `install_crate_args` is set.
- Library mode generates one Go package per uniffi component in the cdylib
  (`livekit_uniffi`, `livekit_datatrack`); the cross-package import in
  `livekit_uniffi` is built from the `go_mod` path in `uniffi.toml`.
- Copying the locally built library into the module root (`go-copy-lib`) is
  the dev delivery mechanism; see Distribution below for how a release flow
  would deliver the library.

## Verify

```bash
cd packages/go && go test ./...
```

## App integration (local development)

Point your app's module at the assembled package with a `replace` directive:

```bash
go mod edit -replace github.com/livekit/livekit-uniffi-go=<path to packages/go>
go mod tidy
```

or add both to a `go.work` workspace. Re-run `cargo make go-package` to
rebuild after Rust-side changes.

The module path is declared in two places that must stay in sync: `GO_MODULE`
in `Makefile.toml` (baked into `go.mod`) and `go_mod` in `uniffi.toml`
(baked into generated import paths).

## Distribution (not yet implemented)

TODO
