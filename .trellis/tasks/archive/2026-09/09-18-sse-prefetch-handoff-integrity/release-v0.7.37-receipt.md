# v0.7.37 release receipt

## Authorization and candidate gate

The user requested committing/pushing the fix, waiting for successful GitHub CI, then publishing a new tag. The existing patch release sequence selects `v0.7.37` after verified latest stable `v0.7.36`. Deployment is not included.

Initial candidate `0c4b54955833383867318fa1751da0c1fa01310f` was pushed to `origin/master`. Rust CI [35304028173](https://github.com/ZipperCode/Aether/actions/runs/35304028173) ran the full gateway suite: 5,627 passed, 1 failed, 3 skipped. All four new prefetch handoff tests passed. The sole failure was `gateway_production_body_collection_stays_bounded`, which scanned the new sibling test file and rejected `to_bytes(response.into_body(), usize::MAX)`.

No tag was created for the failed candidate. The test helper now uses an explicit 128 KiB response collection limit, sufficient for the approximately 75 KB largest fixture and SSE overhead. The architecture scanner, production prefetch fix, runtime limits, and assertion coverage remain unchanged. A fresh complete CI run on the updated candidate is required before tagging.

The existing local test executable reran the unchanged architecture test against current on-disk sources: 1 passed. Focused rustfmt and `git diff --check` passed. This source-scanning check needs no recompilation; execution of the fixture's new cap is delegated to the fresh full GitHub CI build.

## Certified source and tag

- Certified source: `44ba201ff6a851b41ef3482eebcbd40c3d9fece7` (includes the production fix `f18dad7ab` and explicit fixture body limit).
- Rust CI [35305248197](https://github.com/ZipperCode/Aether/actions/runs/35305248197): completed/success, 18/18 jobs, exact source SHA. Gateway result: 5,628 passed, 3 skipped, 0 failed; its log explicitly passes all four new handoff tests and `gateway_production_body_collection_stays_bounded`.
- Annotated tag `v0.7.37` was created only after the exact-SHA success. Tag object `29cedfc1e9a6337a5c156ce183330c45c867ca06` peels to the certified source both locally and on origin.

## Release verification

- Release Aether [35306305644](https://github.com/ZipperCode/Aether/actions/runs/35306305644): completed/success, 8/8 jobs, exact tag/source.
- [GitHub Release v0.7.37](https://github.com/ZipperCode/Aether/releases/tag/v0.7.37): published 2026-09-18 04:34:21 UTC, non-draft, non-prerelease, confirmed latest stable.
- All six assets were downloaded; local SHA-256 and file size matched GitHub release metadata. Both tarball hashes also matched `SHA256SUMS`.

| Asset | Bytes | SHA-256 |
| --- | ---: | --- |
| aether-v0.7.37-linux-amd64.tar.gz | 46145018 | eb160631f46795576eaa3ce292633d4bbd87ac1d6b562864759e68409378c8b2 |
| aether-v0.7.37-linux-arm64.tar.gz | 42546032 | 35853f24ec288ea0b15f095a7c72e39b88bfee69980c6da2b0397a1d5407f2da |
| aether-vscodex-0.4.0.vsix | 328733 | 8ecd6014a3ecbc7834d03568be40eb4873175732d6a86c5c182e0fc17f11bb55 |
| AETHER_RELEASE_PROVENANCE.sigstore.json | 11196 | 9da455cf72088d6d224093d95faa9921623b6f7e600fa5480946ab622e18e1c5 |
| install.sh | 92886 | ffe64176346e46b5a6663052f39634ce0101c9427d79c379b82017db0f9fb1c2 |
| SHA256SUMS | 200 | 2f7b6642574f3d8939269168a66303c91395067709acfdb2b5de1c3221adb63f |

Both tarballs contain 256 entries under their correct single version/architecture root; required executable, frontend index, installer/updater, Compose files, environment example, key generator, README and license are present. VSIX reports 0.4.0 and includes `extension/node_modules/ws/index.js`. The installer pins the new tag. Archives were inspected without installing or executing their contents.

## Provenance and image

`gh attestation verify` succeeded with the downloaded bundle and explicit `--repo ZipperCode/Aether`, `--signer-workflow ZipperCode/Aether/.github/workflows/release.yml`, `--source-ref refs/tags/v0.7.37`, `--source-digest 44ba201ff6a851b41ef3482eebcbd40c3d9fece7` and JSON output. The verified statement binds the exact source/tag and release run. Its four subjects (both tarballs, installer, checksum file) match the downloaded hashes. VSIX and the bundle itself are checked against GitHub asset digests, not claimed as signed subjects.

- Image: `ghcr.io/zippercode/aether:0.7.37`.
- OCI index: `sha256:b4f3782cdaf0317335fb9b9ae6fb96d678ff241e95c829cbb476ec0525baacfa`.
- linux/amd64: `sha256:db70927f59f7a7df12d2cec31d6245dfd7faec1a67a8430dfa6cbf3168752ff8`.
- linux/arm64: `sha256:b031cd543f308895a1348c1b19144d11a21d2645b5772e4041b62464f20434d3`.
- Two `unknown/unknown` index entries are explicitly annotated `attestation-manifest`, each referencing the matching platform manifest.

No deployment, service restart or live inference was performed. The Pages workflow remains absent. Post-release documentation does not change or move the certified tag.
