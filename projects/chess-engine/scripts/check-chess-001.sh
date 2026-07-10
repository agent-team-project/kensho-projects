#!/usr/bin/env bash
set -euo pipefail

cargo metadata --format-version 1 --no-deps >/dev/null
cargo fmt --check
cargo test --workspace
