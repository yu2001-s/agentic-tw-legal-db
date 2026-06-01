#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "usage: scripts/set-publish-urls.sh https://github.com/OWNER/REPO [branch]" >&2
  exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_URL="${1%.git}"
REPO_URL="${REPO_URL%/}"
BRANCH="${2:-main}"

case "$REPO_URL" in
  https://github.com/*/*) ;;
  *)
    echo "repository URL must be a public GitHub HTTPS URL, for example https://github.com/OWNER/REPO" >&2
    exit 2
    ;;
esac

PREFIX="$(git -C "$ROOT" rev-parse --show-prefix 2>/dev/null || true)"
PREFIX="${PREFIX%/}"

python3 - "$ROOT/.codex-plugin/plugin.json" "$REPO_URL" "$BRANCH" "$PREFIX" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
repo_url = sys.argv[2]
branch = sys.argv[3]
prefix = sys.argv[4].strip("/")

def github_path(kind, relpath=""):
    parts = [repo_url, kind, branch]
    if prefix:
        parts.append(prefix)
    if relpath:
        parts.append(relpath)
    return "/".join(part.strip("/") for part in parts)

data = json.loads(path.read_text())
interface = data.setdefault("interface", {})
interface["websiteURL"] = github_path("tree")
interface["privacyPolicyURL"] = github_path("blob", "docs/PRIVACY.md")
interface["termsOfServiceURL"] = github_path("blob", "docs/TERMS.md")
path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")
PY

echo "Updated .codex-plugin/plugin.json with marketplace URLs for $REPO_URL ($BRANCH)"
