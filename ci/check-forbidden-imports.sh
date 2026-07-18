#!/usr/bin/env sh
# engine-core must contain zero browser APIs (final spec §0 Rule 1 /
# CLAUDE.md invariant). CI fails if any browser binding is referenced.
set -eu
if grep -rnE 'web_sys|js_sys|wasm_bindgen' crates/engine-core/src crates/engine-core/Cargo.toml; then
  echo 'FORBIDDEN: browser API reference found in engine-core' >&2
  exit 1
fi
echo 'engine-core clean: no web_sys/js_sys/wasm_bindgen'
