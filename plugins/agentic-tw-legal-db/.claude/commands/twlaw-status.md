---
allowed-tools: Bash(twlaw:*), Bash(scripts/install.sh:*), Bash(scripts/doctor.sh:*)
description: Check twlaw installation, source coverage, and agent guidance.
---

Run:

```bash
twlaw --version || scripts/install.sh
twlaw sources status --json
twlaw sources gaps --json
twlaw agent guide --json
```

Summarize whether the CLI is installed, whether default workflows require external API credentials, which sources are implemented, and which official-source gaps remain.
