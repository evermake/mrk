---
name: fix-issue
description: Workflow for fixing a GitHub issue.
---

Fix the GitHub issue $ARGUMENTS:
1. Gather context using `gh` CLI and make sure the task is clear
2. Create a separate branch off `main` following the format: `<no>-summary` (e.g. `4-cross-block-selection`)
3. Work on issue
4. When finished and no blockers left: commit, push a new branch, open a PR

Notes:
- Never commit or push directly to `main`
- Ask me when need to make a non-obvious decision or when struggle (missing tools, permissions, manual verification, etc.)
