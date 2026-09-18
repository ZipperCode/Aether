# v0.7.37 release receipt

## Authorization and candidate gate

The user requested committing/pushing the fix, waiting for successful GitHub CI, then publishing a new tag. The existing patch release sequence selects `v0.7.37` after verified latest stable `v0.7.36`. Deployment is not included.

Initial candidate `0c4b54955833383867318fa1751da0c1fa01310f` was pushed to `origin/master`. Rust CI [35304028173](https://github.com/ZipperCode/Aether/actions/runs/35304028173) ran the full gateway suite: 5,627 passed, 1 failed, 3 skipped. All four new prefetch handoff tests passed. The sole failure was `gateway_production_body_collection_stays_bounded`, which scanned the new sibling test file and rejected `to_bytes(response.into_body(), usize::MAX)`.

No tag was created for the failed candidate. The test helper now uses an explicit 128 KiB response collection limit, sufficient for the approximately 75 KB largest fixture and SSE overhead. The architecture scanner, production prefetch fix, runtime limits, and assertion coverage remain unchanged. A fresh complete CI run on the updated candidate is required before tagging.

The existing local test executable reran the unchanged architecture test against current on-disk sources: 1 passed. Focused rustfmt and `git diff --check` passed. This source-scanning check needs no recompilation; execution of the fixture's new cap is delegated to the fresh full GitHub CI build.

## Release verification

Pending the new exact-SHA CI gate, annotated tag, release workflow and asset/provenance/image verification.
