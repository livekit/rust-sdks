#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

# A standalone pnpm project — its @livekit/uniffi dependency is a plain `link:`,
# not a pnpm workspace member — so it needs its own copy of the shared
# supply-chain policy rather than inheriting one from a parent workspace. pnpm
# follows the link into the built package, whose tsup pulls esbuild.
cp ../pnpm-workspace.common.yaml pnpm-workspace.yaml

# `pnpm install` only materialises the link; nothing third-party is fetched here.
# node runs the TypeScript itself, so there is no tsx/typescript to pin.
pnpm install
node index.ts
