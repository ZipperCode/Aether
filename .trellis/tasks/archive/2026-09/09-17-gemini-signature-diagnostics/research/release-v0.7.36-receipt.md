# v0.7.36 release receipt

## Source and gates
- Repository: `ZipperCode/Aether`, branch `master`.
- Certified source: `b020cf17040401ecb276ea0bd9a820cd02f32d9b` (includes signature fix `abe0965f5` and usage retention fix `8e9e1836c`).
- Rust CI push run [35200621468](https://github.com/ZipperCode/Aether/actions/runs/35200621468): completed successfully, 18/18 jobs, exact source SHA.
- Annotated tag `v0.7.36` created only after that success. Tag object `0383e9d21d88211f310c5b44c3e23c2a47f3e1ff` peels to the certified source locally and remotely.
- Release Aether run [35202466965](https://github.com/ZipperCode/Aether/actions/runs/35202466965): completed successfully, 8/8 jobs, same tag/source.
- [GitHub Release](https://github.com/ZipperCode/Aether/releases/tag/v0.7.36): latest stable, non-draft, non-prerelease, published 2026-09-17 09:16:02 UTC.

## Downloaded asset verification
All six current assets were downloaded locally and SHA-256 matched GitHub release metadata. Both tarball hashes also matched `SHA256SUMS`.

| Asset | Bytes | SHA-256 |
| --- | ---: | --- |
| aether-v0.7.36-linux-amd64.tar.gz | 46144581 | 06f9a04cc7e821878c6b32a42fa6e02e22806b3032ed56c832251c390ee78261 |
| aether-v0.7.36-linux-arm64.tar.gz | 42547471 | aced07f47c812337e1f12f65d8e27bd9c73609a924159ce8a8cdcb02e9a5d4f8 |
| aether-vscodex-0.4.0.vsix | 328733 | 094b847b264d0b5beb69f6e30c98e9eceb5ca5bf5703e5f4b4b6431811814256 |
| AETHER_RELEASE_PROVENANCE.sigstore.json | 11055 | 9c0f8f33ea37259fbcc4f972e94248c8e17b4afa80405be5bbc09dce0de85c77 |
| install.sh | 92886 | 74fa90a0cd885e1ccfac35cd74d68322661e5f7c30ed24fe21eab9bc45dafb88 |
| SHA256SUMS | 200 | 730410353bbf095adcab24d94696c47c38284563057d7930800e425f8746a725 |

Both tarballs contain 256 entries under their expected single version/architecture root. Required binary, frontend index, installer/updater, Compose files, environment example, key generator, README and license exist. VSIX reports 0.4.0 and includes `extension/node_modules/ws/index.js`. Archives were inspected, not installed or executed.

## Provenance and image
`gh attestation verify` with the downloaded bundle, `--repo ZipperCode/Aether`, `--signer-workflow ZipperCode/Aether/.github/workflows/release.yml`, `--source-ref refs/tags/v0.7.36` and `--source-digest b020cf17040401ecb276ea0bd9a820cd02f32d9b` verified the signed release package subjects. The signed statement lists both tarballs, `install.sh` and `SHA256SUMS`, matching their downloaded hashes. The VSIX and bundle itself are verified by GitHub asset digest, not falsely claimed as additional signed subjects.

- Image: `ghcr.io/zippercode/aether:0.7.36`.
- OCI index: `sha256:ce10e5c62c3f71ab70f7363e27aff5b21752e4beac56a807ab8cb81ce471e299`.
- linux/amd64 manifest: `sha256:61d6c5961dea283487f60ba40b7b07635a5207a28f31ad512944a3f0c59a86a8`.
- linux/arm64 manifest: `sha256:b6f102ae8324559d2798a17baee562d860417a10d3bafcb02fcd6aaf65edbed2`.
- Two `unknown/unknown` entries are explicitly annotated `attestation-manifest` and reference those platform manifests.

## Scope
No server deployment, restart, live inference or client history edits. GitHub Pages remains disabled (API 404; no Pages workflow). Post-release documentation commits do not alter or move the certified tag.
