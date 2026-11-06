# Execution

1. Inspect exact PR diffs and applicable specs, establish disjoint ownership.
2. Adapt #893/#898 protocol behavior and tests; adapt #899 audit compaction and localized setting copy/tests; parent ports #897 TTL and tests.
3. Owners skip mid-flight checks and run focused tests once coherent; parent reuses their evidence and runs integration fmt/diff plus actual setting surface observation. No full-suite or real external providers.
4. Review final diff, update existing specs only where necessary, commit/push, archive scoped task and record outcome.
