#!/usr/bin/env bash
# Everything that must pass before pushing: host formatting, lints and tests (including the wasm target),
# the wasm build and its smoke test, and, when the ESP toolchain is installed, the firmware build. The
# firmware build matters on its own: the Xtensa compiler has failed on code that every host check passed.
set -euo pipefail

eyes="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$eyes"
step() { printf '\n==> %s\n' "$*"; }

step "host: fmt, clippy, tests"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p eyes-web --target wasm32-unknown-unknown --locked -- -D warnings
cargo test --workspace --locked

step "wasm: build and smoke test"
./eyes-web/build-wasm.sh
if command -v node >/dev/null 2>&1; then
  node eyes-web/web/smoke.mjs
else
  echo "node not found: skipping the wasm smoke test"
fi

step "firmware: cargo +esp build --release"
export_esp="${ESP_EXPORT:-$HOME/export-esp.sh}"
if [[ -f "$export_esp" ]] && rustup toolchain list | grep -q '^esp'; then
  # shellcheck source=/dev/null
  source "$export_esp"
  (cd firmware && cargo +esp fmt --check && cargo +esp clippy --release -- -D warnings && cargo +esp build --release)
else
  echo "ESP toolchain not found (espup install, then $export_esp): skipping the firmware build"
fi

step "all checks passed"
