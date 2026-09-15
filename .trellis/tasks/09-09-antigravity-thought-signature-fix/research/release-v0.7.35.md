# v0.7.35 release evidence

## Source and authorization

- User request: commit local changes, push, wait for GitHub CI, then publish a new tag.
- Work commit: `52df0e6243683f0d85eb84a32b03048f480c4bec` — 修复 Antigravity 签名恢复与终态记录.
- Annotated tag: `v0.7.35`; tag object `7b7e019c4b7e7b036f5162fa6302291dfb9af3ce`; remote dereferenced target matches the work commit.
- Tag creation and push occurred only after the exact source SHA passed all Rust CI jobs.

## Current verification: passed

- Independent source review and scoped Rust formatting: passed; see `release-check-receipt.md` for source fingerprints and fixes.
- [Rust CI 34912705285](https://github.com/ZipperCode/Aether/actions/runs/34912705285): completed, success, **18/18 jobs**, head SHA equals the work commit.
- Relevant executed regressions: **10 gateway signature-recovery tests, 8 provider Antigravity request tests, 1 data-contract diagnostic projection test**, all passed. Includes the new cancellation snapshot regression and authenticated sync/stream/admin tunnel tests.
- Gateway suite: **5,624 passed, 3 skipped**; workspace-rest suite: **3,825 passed, 30 skipped**; data suite: **376 passed, 1 skipped**; PostgreSQL adapter suite: **238 passed, 28 skipped**. Skipped tests are not counted as passing. Dedicated PostgreSQL smoke jobs also passed.
- Frontend type-check/tests/build, all Clippy jobs, format and shell fixtures passed.
- The old local Docker builder is absent; current compile/test conclusions come from GitHub CI, not historical Docker counts.

## Release verification: passed

- [Release Aether 34913719222](https://github.com/ZipperCode/Aether/actions/runs/34913719222): completed, success, **8/8 jobs**, `v0.7.35` at the same source SHA.
- [GitHub Release v0.7.35](https://github.com/ZipperCode/Aether/releases/tag/v0.7.35) was published at `2026-09-15T00:52:40Z`, is non-draft/non-prerelease, and is the verified latest stable release.
- Downloaded all six assets. Every actual local SHA-256 and byte count matches GitHub's uploaded asset metadata. Both `SHA256SUMS` entries match the downloaded tarballs; both archives contain the gateway binary and `frontend/index.html` in the versioned bundle root.
- `gh attestation verify install.sh --repo ZipperCode/Aether --signer-workflow ZipperCode/Aether/.github/workflows/release.yml --source-digest 52df0e6243683f0d85eb84a32b03048f480c4bec --bundle AETHER_RELEASE_PROVENANCE.sigstore.json` exited **0**. All four attested subjects (both tarballs, installer and checksums) match the downloaded bytes. The signed source is `git+https://github.com/ZipperCode/Aether@refs/tags/v0.7.35`, `gitCommit=52df0e6243683f0d85eb84a32b03048f480c4bec`.
- `docker buildx imagetools inspect ghcr.io/zippercode/aether:0.7.35` exited **0** and confirmed `linux/amd64` plus `linux/arm64`; the two `unknown/unknown` entries are explicitly marked attestation manifests.
- No production deployment or live inference was performed by this release task. The downloaded Linux binaries were inspected as release artifacts, not executed on Windows.

### Downloaded asset hashes

| Asset | Bytes | SHA-256 |
| --- | ---: | --- |
| `aether-v0.7.35-linux-amd64.tar.gz` | 46006840 | `4f6292ff2a7e3c33cbd2b745df964400e5dd27986fe3965b6d9ab42ba5d31beb` |
| `aether-v0.7.35-linux-arm64.tar.gz` | 42196529 | `9229921daf746eb1b6255b7bca88220d9ab5fea68738a29c8a3e06072c98e253` |
| `aether-vscodex-0.4.0.vsix` | 328733 | `b83eebca60f62f5ba6a0bbbd396d5714fd69223e67923700ac08dc92dd3b94bb` |
| `AETHER_RELEASE_PROVENANCE.sigstore.json` | 11196 | `a87b296f2e56342d7661a019a6fbc61af37c86bc25845b5c5f51219e6a986f9f` |
| `install.sh` | 92886 | `cbbf26a0c5731f8cad0c21a393ad10c4eb9a8ae22e2a9bd76b2f4ba905c3ea0b` |
| `SHA256SUMS` | 200 | `3a2c4b22bf30ba4f56ac37116870128c56628848d3a2aeee83fa07d23a6bd448` |

### Published image digests

- Index: `sha256:0ef6acc62a6d28fda28c95cbaae6a5163cb39a87135bf4f2dc90d33e59807375`.
- Linux amd64: `sha256:9a34f83523714e56ec76fe4c907a4cc5962c7aea5a4be5bfb493e51fe87a8379`.
- Linux arm64: `sha256:a76e8afa9334d21064683c3c55ece9e8685d20c80b422bac9501b9ed6329e54a`.
