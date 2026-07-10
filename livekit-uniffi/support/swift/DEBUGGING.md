---
name: uniffi-swift-debug
description: Debug Rust code (livekit-uniffi) through Swift consumers with full Rust stack traces in lldb/Xcode. Use when Rust frames are missing from Xcode/lldb backtraces, when setting breakpoints in .rs files from a Swift app, when building a local debug LiveKitUniFFI xcframework, when linking client-sdk-swift to a local rust-sdks checkout, or when creating debug workspaces or headless lldb test rigs for the UniFFI boundary.
---

# Debugging Rust through Swift (UniFFI)

The published `livekit-uniffi-xcframework` is a release build with `strip = "symbols"`
(see `livekit-uniffi/Cargo.toml`), so lldb has nothing to symbolicate: Xcode shows only
the generated UniFFI Swift glue and bare addresses for Rust. **A locally built debug
xcframework has full DWARF** — lldb then shows mixed Swift/Rust backtraces, Rust source
listings, stepping, and variables (`frame variable` prints Rust values).

Debug info stays in the Rust `target/` object files (referenced via OSO stabs), so:
keep the rust-sdks checkout/target dir around, and rebuild the xcframework after any
Rust change. Quick check that a build is debuggable: `nm -a <binary> | grep -c ' OSO '`
(> 0 means DWARF refs present; a stripped release build has 0 and ~1k symbols vs ~14k).
The binary lives inside the xcframework at
`RustLiveKitUniFFI.xcframework/macos-arm64_x86_64/RustLiveKitUniFFI.framework/RustLiveKitUniFFI`
(platform dir varies, e.g. `ios-arm64`).

## Repo layout, placeholders, and worktrees

Placeholders used throughout. **Resolve them from session context — never assume.**
Look at what is currently being worked on: the cwd, checkouts or worktrees the user
has named, branches already containing the changes under debug. Ask if it's ambiguous.

- `<rust-sdks>` — the rust-sdks working copy whose Rust code is being debugged. If the
  session is already inside one (main checkout or a worktree), that's it.
- `<client-sdk-swift>` — the client-sdk-swift working copy whose manifests will be
  edited in Recipe 2. If the user is already working in one, use it; only create a
  fresh worktree (Recipe 2) when there is none, to avoid dirtying a checkout that
  isn't part of the current work.
- Default layout when nothing is in context: repos cloned side by side — `rust-sdks/`,
  `client-sdk-swift/`, `swift-example/` (LiveKitExample app) share a parent directory.
  So from the main rust-sdks checkout, the Swift SDK is `../client-sdk-swift` and the
  example app is `../swift-example`.

Worktree rules — these apply to **every cross-repo link** below, whenever any working
copy involved is a git worktree:

- Resolve sibling repos from the **main checkout**, never from a worktree path
  (`../client-sdk-swift` does not exist next to `.claude/worktrees/<name>`). From
  inside any worktree, the main repo is `$(dirname "$(git rev-parse --git-common-dir)")`.
- Write **absolute paths** when wiring repos together — SPM `path:` dependencies and
  `.xcworkspace` FileRefs. Relative paths break as soon as either side is a worktree.
- The debug info is tied to the exact working copy that built it: lldb resolves DWARF
  through OSO references into its `target/`. Build the xcframework, point manifests at
  it, and reference Rust sources in Xcode all from the **same** `<rust-sdks>`; don't
  move or delete it mid-session.
- If (and only if) you created a worktree for this session, clean it up when done:
  `git worktree remove <path>` (`--force` if the uncommitted manifest edits from
  Recipe 2 are still present, which is expected). Never remove a working copy the
  user was already using.

## Recipe 1 — build a local debug xcframework

```sh
cd <rust-sdks>/livekit-uniffi
cargo make swift-package-debug                            # macOS only — fastest
SPM_PLATFORMS="macos ios" cargo make swift-package-debug  # + iOS sim/device
```

This builds an unstripped debug dylib (full DWARF) for the selected platforms only —
no nightly toolchains needed — and produces the same path-consumable package as the
dev-profile `swift-package` task. cargo-make installs `cargo-swift`/`tera` pins
automatically on first run. For all Apple platforms instead: `cargo make swift-package`
(needs nightly + rust-src for tvOS/visionOS — slow).

Output: `<rust-sdks>/livekit-uniffi/packages/swift/LiveKitUniFFI/` — an SPM package
wrapping the debug `RustLiveKitUniFFI.xcframework`.

## Recipe 2 — point client-sdk-swift at the local package

Pick `<client-sdk-swift>` from context first (see placeholders above). Only if no
working copy is part of the current work, create one — don't dirty the main checkout:

```sh
CSS_MAIN=<...>/client-sdk-swift   # sibling of the main rust-sdks checkout
git -C "$CSS_MAIN" worktree add -b uniffi-local-debug \
  "$CSS_MAIN/.claude/worktrees/uniffi-local-debug" main
```

In `<client-sdk-swift>`, replace the released dependency in **both** `Package.swift` and
`Package@swift-6.2.swift`:

```swift
// before:
.package(url: "https://github.com/livekit/livekit-uniffi-xcframework.git", exact: "X.Y.Z"),
// after — absolute path to Recipe 1 output:
.package(name: "livekit-uniffi-xcframework", path: "<rust-sdks>/livekit-uniffi/packages/swift/LiveKitUniFFI"),
```

Keep the `name:` parameter — `.product(..., package: "livekit-uniffi-xcframework")`
references it. Don't commit this change.

Heads-up: if an `.xcodeproj` app will consume this working copy through a workspace
folder reference (Recipe 4), the override matches by directory **basename** — a
worktree named anything but `client-sdk-swift` needs the symlink from Recipe 4's
caveats. Explicit `path:` dependencies (Recipe 3) don't care.

## Recipe 3 — headless CLI sample + lldb batch (agentic verification)

A minimal macOS CLI exercising Swift → SDK → UniFFI → Rust, debuggable without any GUI.
Create a package (e.g. under `/tmp`):

```swift
// Package.swift
// swift-tools-version:6.1
import PackageDescription
let package = Package(
    name: "lk-uniffi-debug-sample",
    platforms: [.macOS(.v14)],
    dependencies: [.package(name: "LiveKit", path: "<client-sdk-swift>")],   // absolute path
    targets: [.executableTarget(name: "lk-uniffi-debug-sample",
                                dependencies: [.product(name: "LiveKit", package: "LiveKit")])]
)
```

```swift
// Sources/lk-uniffi-debug-sample/main.swift
import Foundation
import LiveKit

// Unsigned JWT: token_claims_from_unverified parses without verifying the signature.
func base64URL(_ json: String) -> String {
    Data(json.utf8).base64EncodedString()
        .replacingOccurrences(of: "+", with: "-")
        .replacingOccurrences(of: "/", with: "_")
        .replacingOccurrences(of: "=", with: "")
}
let header = base64URL(#"{"alg":"HS256","typ":"JWT"}"#)
let payload = base64URL(#"{"exp":4102444800,"iss":"demo-key","sub":"alice","name":"Alice"}"#)
let token = "\(header).\(payload).c2lnbmF0dXJl" // cspell:disable-line

let response = TokenSourceResponse(serverURL: URL(string: "wss://example.livekit.cloud")!,
                                   participantToken: token)
// Swift app → LiveKit SDK → UniFFI glue → Rust (livekit-uniffi → livekit-api → jsonwebtoken)
print("dispatchesAgent: \(response.dispatchesAgent())")
```

```text
# lldb-demo.txt
breakpoint set --name token_claims_from_unverified
breakpoint set --name from_unverified
run
bt
frame variable token
continue
bt 20
continue
```

```sh
swift build
lldb --batch -s lldb-demo.txt ./.build/debug/lk-uniffi-debug-sample
```

Success looks like frames spanning `livekit_api::access_token::Claims::from_unverified`
→ `livekit_uniffi::access_token::token_claims_from_unverified` → `uniffi_core` rustcalls
→ generated `livekit_uniffi.swift` → `TokenSource.swift` → `main.swift`, each with
file:line. If Rust frames show as bare addresses, the dylib is stripped — redo Recipe 1
and confirm the worktree manifests point at it (`swift package resolve` output).

Variant — uniffi functions the SDK doesn't call: client-sdk-swift only consumes part of
the bindings (e.g. token parsing, not `tokenGenerate`). To debug the rest, skip
Recipe 2 and depend on the Recipe 1 package directly:

```swift
dependencies: [.package(path: "<rust-sdks>/livekit-uniffi/packages/swift/LiveKitUniFFI")],
// target dependency: .product(name: "LiveKitUniFFI", package: "LiveKitUniFFI")
```

then `import LiveKitUniFFI` and call the generated glue (see the public API in the
package's `Sources/LiveKitUniFFI/livekit_uniffi.swift`) — same boundary, same
backtraces, smaller dependency graph.

## Recipe 4 — Xcode debug workspace

An `.xcworkspace` whose `contents.xcworkspacedata` references (1) the app, (2) the
client-sdk-swift worktree (folder reference = local package override, same pattern as
`swift-example`'s `LiveKitExample-dev.xcworkspace`), and (3) the Rust source dirs so
`.rs` files are browsable and gutter-breakpointable. All FileRefs absolute:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Workspace version = "1.0">
   <FileRef location = "absolute:<sample package dir>"></FileRef>   <!-- or the app .xcodeproj -->
   <FileRef location = "absolute:<rust-sdks>/livekit-uniffi/src"></FileRef>
   <FileRef location = "absolute:<rust-sdks>/livekit-api/src"></FileRef>
</Workspace>
```

For the example app: copy `<...>/swift-example/LiveKitExample-dev.xcworkspace`'s
pattern (FileRef to the client-sdk-swift worktree + `LiveKitExample.xcodeproj`) and add
the Rust src FileRefs.

Caveats:
- When the app is an `.xcodeproj` with a **remote** dependency on client-sdk-swift, the
  folder reference only overrides it if the directory's basename equals the package
  identity: resolution fails with "identity 'client-sdk-swift' doesn't match override's
  identity (directory name) '<dir>'". A worktree named differently still works through
  a symlink — `ln -s <client-sdk-swift worktree> <somewhere>/client-sdk-swift` — and
  pointing the FileRef at the symlink. Verify headlessly before opening Xcode:
  `xcodebuild -resolvePackageDependencies -workspace <ws> -scheme <any listed scheme>`
  should resolve `LiveKit` to the local path and `LiveKitUniFFI` to the Recipe 1 output.
- Xcode refuses to open the same SPM package in two workspaces — close other windows.
- Rust folder refs must point at the **same `<rust-sdks>`** that built the
  xcframework; file:line breakpoints match DWARF paths absolutely.
- The run destination must match a platform the xcframework was built for: the Recipe 1
  fast path is macOS-only, so use a Mac destination — for the iOS simulator, rebuild
  with `SPM_PLATFORMS="macos ios"` and re-resolve packages.
- An app project's schemes may not be shared — headless `xcodebuild -list` then won't
  show them. Xcode auto-creates them on first open; pick the app scheme in the GUI, or
  write one into the workspace for headless use (below).
- If the Debug Navigator hides frames, drag its filter slider (bottom) to the right.

Headless verification of the workspace (agentic — no GUI): if the app project has no
shared scheme, write one at `<ws>.xcworkspace/xcshareddata/xcschemes/<App>.xcscheme`.
Its `BuildableReference` needs `BlueprintIdentifier` (the app's `PBXNativeTarget`
object ID — grep `project.pbxproj` for `PBXNativeTarget "<App>"`), `BuildableName`
(`<App>.app`), `BlueprintName` (the target name), and `ReferencedContainer`
(`container:` + absolute path to the `.xcodeproj`). Then:

```sh
# package graph: LiveKit → local working copy, LiveKitUniFFI → Recipe 1 output
xcodebuild -resolvePackageDependencies -workspace <ws> -scheme <App>
# compile/link check without the user's signing identity:
xcodebuild build -workspace <ws> -scheme <App> -destination 'platform=macOS,arch=arm64' CODE_SIGNING_ALLOWED=NO
```

## Gutter breakpoints in .rs files (one-time UTI fix)

Xcode only enables the breakpoint gutter for files conforming to `public.source-code`;
`.rs` maps to a dynamic UTI by default. Check:

```sh
swift -e 'import UniformTypeIdentifiers; let t = UTType(filenameExtension: "rs"); print(t?.identifier ?? "nil", t?.conforms(to: .sourceCode) ?? false)'
```

If `false`: **ask the user first** (system-wide change), then create
`~/Applications/RustSourceUTI.app/Contents/Info.plist` — a plist-only bundle with
`CFBundleIdentifier: io.livekit.rust-source-uti` and a `UTExportedTypeDeclarations`
entry exporting `org.rust-lang.rust-source` (conforms to `public.source-code`,
extension `rs`) — and register it:

```sh
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f ~/Applications/RustSourceUTI.app
```

Restart Xcode. Undo: `lsregister -u ~/Applications/RustSourceUTI.app && rm -rf ~/Applications/RustSourceUTI.app`.

## Breakpoint cheat sheet

| Goal | How |
| --- | --- |
| Break on a uniffi-exported fn | `br set -n token_claims_from_unverified` (or Xcode Symbolic Breakpoint) |
| Break on any Rust method | `br set -n <base name>`, e.g. `br set -n from_unverified` |
| Break by file:line | `br set -f access_token.rs -l <line>` (basename matching works) |
| Third-party crates | sources live under `~/.cargo/registry/src/...` — open those exact files |
| Add breakpoint without gutter | cursor on line, ⌘\ |

Symbolic-name breakpoints survive Rust edits; file:line breakpoints may drift — look
up current lines with grep rather than hardcoding. Basename matching also binds in
**every** file with that name — both livekit-uniffi and livekit-api have an
`access_token.rs`, so expect resolved locations (and stops) in each.

## What you can do with this skill

Agentic (headless — Claude does it end-to-end and reports the backtrace):
- "Verify my Rust change in livekit-uniffi is actually hit when the Swift SDK parses a
  token" → Recipes 1–3, breakpoint on the changed fn, report `bt` + variables.
- "Why does `tokenClaimsFromUnverified` throw for this token?" → Recipe 3 with the
  user's token string, step through `Claims::from_unverified`, inspect Rust state.
- "Set up Rust debugging for client-sdk-swift against my local rust-sdks" → Recipes
  1–2, then verify with Recipe 3 before handing over.

Manual (Xcode GUI — Claude sets up, the user debugs):
- "Create a debug workspace for the example app with my local Rust build" → Recipes 1,
  2, 4 (+ UTI fix if needed); user clicks breakpoints in `.rs` files and runs the app.
- "I can't see Rust frames in Xcode" → diagnose (OSO check), rebuild debug xcframework,
  relink, remind about the frame-filter slider and symbolic breakpoints.
