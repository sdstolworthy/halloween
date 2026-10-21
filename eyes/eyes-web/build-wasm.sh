#!/usr/bin/env bash
# Build the release wasm for the browser emulator into web/eyes_web.wasm (gitignored; the Dockerfile
# runs this too). --out writes elsewhere. Uses its own target dir so it never fights a host build.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
workspace="$(cd "$here/.." && pwd)"
target_dir="${CARGO_TARGET_DIR:-$workspace/target/web}"
out="$here/web/eyes_web.wasm"

case "${1:-}" in
  --out) out="${2:?--out needs a path}" ;;
  "") ;;
  *) echo "usage: $0 [--out PATH]" >&2; exit 2 ;;
esac

CARGO_TARGET_DIR="$target_dir" cargo build --release --locked -p eyes-web \
  --manifest-path "$workspace/Cargo.toml" --target wasm32-unknown-unknown
mkdir -p "$(dirname "$out")"
cp "$target_dir/wasm32-unknown-unknown/release/eyes_web.wasm" "$out"
echo "wrote $out ($(wc -c < "$out" | tr -d ' ') bytes)"
