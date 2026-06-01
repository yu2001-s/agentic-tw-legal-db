#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to install twlaw" >&2
  exit 2
fi

cargo install --path "$ROOT/crates/twlaw-cli" --locked --force
twlaw --version
