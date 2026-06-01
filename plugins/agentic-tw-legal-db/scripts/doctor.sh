#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "plugin: $ROOT"
cargo --version
rustc --version

if command -v twlaw >/dev/null 2>&1; then
  twlaw --version
else
  echo "twlaw is not installed; run scripts/install.sh" >&2
fi
