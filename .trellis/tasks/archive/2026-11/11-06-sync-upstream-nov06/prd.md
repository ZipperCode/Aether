# Upstream main integration and PR assessment

Merge upstream/main at 0eb323390 into current master, preserving local model resolution, quota admission, native passthrough, streaming diagnostics and Pages-disabled contracts. Then assess PRs #893, #897, #898, followed by #899 against the merged implementation; do not integrate those PRs without further authorization.

Acceptance: all merge conflicts resolved semantically; upstream main is an ancestor; relevant frontend and backend behavior checks pass; observe the merged UI locally; commit and push only the isolated main integration after successful verification. Report concrete PR gaps, risks and recommendations. No production data changes, external provider calls, schema redesign or unrelated cleanup.
