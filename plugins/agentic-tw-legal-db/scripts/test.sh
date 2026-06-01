#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo fmt --all -- --check
cargo test --workspace --locked

VALIDATOR="/Users/shaoyuhuang/.codex/skills/.system/plugin-creator/scripts/validate_plugin.py"
if python3 -c "import yaml" >/dev/null 2>&1; then
  python3 "$VALIDATOR" "$ROOT"
else
  VENV="${TMPDIR:-/tmp}/twlaw-plugin-validator-venv"
  python3 -m venv "$VENV"
  PIP_DISABLE_PIP_VERSION_CHECK=1 "$VENV/bin/python" -m pip install -q PyYAML
  "$VENV/bin/python" "$VALIDATOR" "$ROOT"
fi
