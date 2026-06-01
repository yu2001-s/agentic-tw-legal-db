---
allowed-tools: Bash(scripts/release-check.sh:*), Bash(git status:*), Read
description: Check whether the plugin is ready for GitHub and marketplace publication.
---

Run:

```bash
scripts/release-check.sh
git status --short
```

Then inspect `.codex-plugin/plugin.json`. If marketplace URL fields are missing, explain that they must be stamped after a real GitHub repository URL exists:

```bash
scripts/set-publish-urls.sh https://github.com/OWNER/REPO
scripts/release-check.sh --marketplace
```
