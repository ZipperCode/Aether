# v0.7.32 发布核验

## 已完成门禁

- 首次修复c1e994159已推送；CI34259830393失败后未发布tag，修复首字节记账回归再正常追加提交。
- 发布源码：7eaea44b082d2cdd1e4b0133ee04c83fb7b703ca。
- Rust CI34263635757：https://github.com/ZipperCode/Aether/actions/runs/34263635757，精确SHA匹配，18/18作业completed/success，watch正常exit0。
- annotated tag v0.7.32 对象1a0ebd16df90c08c99f6db522d728d146a24e004，本地/远端解引用均为上述源码SHA。tag只在本次CI成功后创建并正常推送，未覆盖其他tag。
- Release Aether34265118756：https://github.com/ZipperCode/Aether/actions/runs/34265118756，headBranch=v0.7.32，headSha匹配，已确认in_progress。

## 发布核验

- [x] Release全部8个作业成功。
- [x] Release正式发布，6个预期资产非空，摘要与provenance一致。
- [x] GHCR 0.7.32同时包含linux/amd64和linux/arm64。
- [x] 任务自有下载/进程清理，证据留存；随后按Trellis归档，不部署。

## 最终结果

- Rust CI34263635757：2026-09-08T18:32:26Z启动，18:44:54Z完成；Gateway实际5462/5462通过，3 skipped如实保留。此前首事件失败测试此次0.934s通过；完整矩阵18/18成功。
- Release34265118756：headSha与tag源码一致，8/8成功，18:47:22Z首job启动、19:05:26Z末job完成。公开Release于19:05:23Z发布，非draft/非prerelease，latest=v0.7.32。
- 页面：https://github.com/ZipperCode/Aether/releases/tag/v0.7.32。

| 资产 | 字节 | SHA-256 |
| --- | ---: | --- |
| aether-v0.7.32-linux-amd64.tar.gz | 44314014 | c2cc174b493bec6832c55ef8187ba53f6c09b5d0f4bc3a8de0a9411cd0d54c13 |
| aether-v0.7.32-linux-arm64.tar.gz | 40386568 | 5e37a027335e252db57f02f1f2f79ef352bb1d2b79780a160bc2bb282c45132c |
| aether-vscodex-0.4.0.vsix | 328733 | 905909f03488868965df0ad03ec138b638f0778b4a1b8262c6c5125711f89ea0 |
| AETHER_RELEASE_PROVENANCE.sigstore.json | 11133 | 6d71d2640b6d226d69f13df041f3317d653eaefd81f477a8c5ad2d7d1b4ee4ac |
| install.sh | 92886 | d2eb1fd72d675527f87ad4fa72f8acaaa6ec6b621fb2cb372e349e74fc09c199 |
| SHA256SUMS | 200 | bc74bcae72693112053d9faa2fe5ec44e09235825e4009c4cca65c736fba1341 |

四小资产已下载并Get-FileHash匹配GitHub digest。两tar未整包下载，SHA256SUMS、GitHub digest与来源证明subject三者一致；不声称已安装运行。

`gh attestation verify install.sh --repo ZipperCode/Aether --signer-workflow ZipperCode/Aether/.github/workflows/release.yml --bundle AETHER_RELEASE_PROVENANCE.sigstore.json` exit0。statement来源git+https://github.com/ZipperCode/Aether@refs/tags/v0.7.32，gitCommit=7eaea44b082d2cdd1e4b0133ee04c83fb7b703ca，builder=release.yml@refs/tags/v0.7.32；subjects匹配两tar、install.sh、SHA256SUMS摘要。

`docker buildx imagetools inspect ghcr.io/zippercode/aether:0.7.32`成功：

- index：sha256:0c86d1ead55d1fa60bfd456e29e62241cb9d285c5d2e6e6aff17451b1f2bd0c2。
- linux/amd64：sha256:27a3a6d5af68d9e90c4071e9fde3d01ed93087b91c922610865fd4e7d3732b89。
- linux/arm64：sha256:d9aab65534be0c77155b0d9cc997575a8db6bc1d4d832472ebdccb978eca6b20。
- 另两unknown/unknown明确为attestation-manifest，并非缺少运行平台。未pull/启动镜像。

临时目录 C:/Users/Zipper/AppData/Local/Temp/aether-v0.7.32-verify-01a081c4 内四个校验下载及空目录已删除，远端资产可重新下载。红测临时日志已由verification.md保留摘要后删除。CI/release watch均exit0，无本任务存活进程。不改线上配置/DB，不重启、不部署、不移动旧tag。
