# Reviewed upstream sources

- Local baseline: `e34c05e8970d46444c3e1480ef94384521fb1fad`.
- Upstream main: `ba7c9f8b270cce63b0515299076b30129d7d64b4`.
- PR #833: `0486435f16b7db31add34133aa819e8477c81ea0`.
- #824 stale-score change: `7daf355e65708a348d2a8f3db6fbc6e36023f07d`.
- #824 fixed-order change: `01acff077468dcde8271a2efd9b86d420a814ee1`.

The coordinator supplied exact `git show <commit>` output to the researcher
and imported #833 with `git diff --binary upstream/main...upstream/pr-833`
followed by `git apply --3way`. Temporary diff files were removed after review;
these source IDs and the final merge commit preserve the integration provenance.
