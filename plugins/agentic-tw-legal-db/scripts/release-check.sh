#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKETPLACE=false
if [[ "${1:-}" == "--marketplace" ]]; then
  MARKETPLACE=true
elif [[ $# -gt 0 ]]; then
  echo "usage: scripts/release-check.sh [--marketplace]" >&2
  exit 2
fi

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

VALIDATOR="/Users/shaoyuhuang/.codex/skills/.system/plugin-creator/scripts/validate_plugin.py"
if [[ -f "$VALIDATOR" ]]; then
  if python3 -c "import yaml" >/dev/null 2>&1; then
    python3 "$VALIDATOR" "$ROOT"
  else
    VENV="${TMPDIR:-/tmp}/twlaw-plugin-validator-venv"
    python3 -m venv "$VENV"
    PIP_DISABLE_PIP_VERSION_CHECK=1 "$VENV/bin/python" -m pip install -q PyYAML
    "$VENV/bin/python" "$VALIDATOR" "$ROOT"
  fi
else
  echo "Codex plugin validator not found; skipped local validator" >&2
fi

if [[ "$MARKETPLACE" == true ]]; then
  python3 - "$ROOT/.codex-plugin/plugin.json" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
data = json.loads(path.read_text())
interface = data.get("interface", {})
required = ["websiteURL", "privacyPolicyURL", "termsOfServiceURL"]
missing = []
for key in required:
    value = interface.get(key, "")
    if not isinstance(value, str) or not value.startswith("https://github.com/") or "OWNER/REPO" in value:
        missing.append(key)

if missing:
    print("Marketplace URL fields missing or not stamped with a real GitHub URL:", ", ".join(missing), file=sys.stderr)
    print("Run: scripts/set-publish-urls.sh https://github.com/OWNER/REPO", file=sys.stderr)
    sys.exit(2)
PY
fi

echo "Release check passed"
