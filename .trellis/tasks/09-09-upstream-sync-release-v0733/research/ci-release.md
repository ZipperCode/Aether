# 精确 SHA CI 与 v0.7.33 发布证据

## 合并和远端

- 合并提交：`c2c30e60d3543fbde19ebc6e1d9f083033649ee0`，双亲为本地 `4f865ff53518885acf756250fb083ce6a86a50e5` 和上游 `361952ada9e3d01ac180846cee497cd1f55ad1c3`。
- 主worktree已ff-only合回并正常push，origin/master已核验同SHA。本地原3个提交及历史保留。
- 隔离worktree `C:/Users/Zipper/AppData/Local/Temp/aether-upstream-release-20260909` 已在干净且分支全合并后删除，临时分支 `codex/upstream-release-20260909` 已删除。主目录正常Cargo缓存保留，没有删除用户数据。

## 完整 GitHub CI

- https://github.com/ZipperCode/Aether/actions/runs/34313596660。
- `headSha=c2c30e60d3543fbde19ebc6e1d9f083033649ee0`，completed/success，18/18 jobs成功，没有失败或未完成job。
- UTC 2026-09-09 05:07:11创建，05:28:00终态；总20m49s。
- Gateway job102345108673：构建6m33s，nextest761.424s，5502 tests run/5502 passed/3 skipped。
- Frontend job102345108287：227文件/1706测试通过，类型检查和构建成功；Vitest112.37s。
- Workspace Rest3575/3575通过（22 skipped）；Data372/372通过（1 skipped）。不将已有跳过项算成通过。
- 所有Clippy、PostgreSQL烟测、数据特性、适配器及集成job成功。各套可能存在重叠，不汇总成全局独立测试总数。
- 本次合并后的首轮完整CI即通过，没有为新合并追加CI修复push，也未弱化门禁或删测试。

## Tag 与发布（已验证）

- 应用 `v0.7.33` 是确认latest v0.7.32后的下一补丁号；创建前检查本地/远端均不存在。
- annotated tag对象：`b78a5205a09ec8fb371f16f4c935e0895c14a98f`；本地/远端解引用均为已获CI成功的`c2c30e60d3543fbde19ebc6e1d9f083033649ee0`。
- https://github.com/ZipperCode/Aether/actions/runs/34315225804，UTC2026-09-09 05:31:31由tag push触发。
- Release全部8/8 jobs成功，headSha与tag目标一致；05:31:31Z创建，05:48:55Z末job完成，总17m24s。前端53s、VSIX45s、amd64 15m48s、arm64 14m18s、tarballs23s、镜像65s、Release资产16s；并行作业时长不相加。
- 预期资产沿用当前release.yml：两个tar.gz、install.sh、SHA256SUMS、签名provenance及aether-vscodex-0.4.0.vsix；GHCR镜像0.7.33。

## 资产与来源核验

- Release https://github.com/ZipperCode/Aether/releases/tag/v0.7.33 已于2026-09-09 05:48:53Z公开发布，isDraft=false、isPrerelease=false、isLatest=true。

| 资产 | 字节 | SHA-256 |
| --- | ---: | --- |
| aether-v0.7.33-linux-amd64.tar.gz | 44903349 | 8c8c6d61e7ff355f797a1cfd01798e264d2b738bce81939ff4ced034971f9c9f |
| aether-v0.7.33-linux-arm64.tar.gz | 41091085 | fbc85a4778a98c5577d8528c61ca77e1bf9a0e631decf8c4988b93ef28b12f59 |
| aether-vscodex-0.4.0.vsix | 328733 | f6defe5ef6907c8af951763fdea34078c14ec0baac244b87924daf82c8c5fd1b |
| AETHER_RELEASE_PROVENANCE.sigstore.json | 11388 | 7b2e8191e5cb89dee4a709f4222b5e2e1e5b63a3362a763a19cbcf94065de32b |
| install.sh | 92886 | 59c134cf39578c0f1183f6eb971136b4f7a08673b9ca259c80a67844cf108007 |
| SHA256SUMS | 200 | 038d4669a2fe211052d050902ea1dbb0554f11e24d26c978cd88a423d87f9e37 |

- 四项小资产下载到独占系统临时目录，Get-FileHash全部与GitHub digest匹配；两tar包未全量本地下载，使用SHA256SUMS/GitHub服务端digest/已验证签名subject三方一致性核验，不声称本地解压或运行。
- `gh attestation verify install.sh --repo ZipperCode/Aether --signer-workflow ZipperCode/Aether/.github/workflows/release.yml --bundle AETHER_RELEASE_PROVENANCE.sigstore.json --format json` exit0。
- 验证后的statement source `git+https://github.com/ZipperCode/Aether@refs/tags/v0.7.33`，gitCommit=c2c30e60d3543fbde19ebc6e1d9f083033649ee0；subjects包含两个tar包、install.sh、SHA256SUMS并匹配上表。
- 安装脚本SOURCE_REF/VERSION默认均v0.7.33；未执行脚本。
- `docker buildx imagetools inspect ghcr.io/zippercode/aether:0.7.33`成功：OCI index `sha256:3cd999d57d22c7da69d36388fe9694eab2fe2a439a2ce5d9964922fe1390cf6e`。
- linux/amd64 manifest `sha256:f5c6a524ec0665c614856f16ba83b443a7433edb3a9de762095dba62a9a8a1c1`；linux/arm64 manifest `sha256:07c136ed999cd32f6e8038b2b93be8975ba8ae4f58326265f57595b8715369cc`。另两个unknown/unknown为明确的attestation-manifest，不是缺失平台。

## 完成边界

所有监测/验证命令结束；独占临时下载目录和四文件按确切路径清理。保留正常Cargo缓存，worktree/临时分支已删除。仅新增应用tag和工作流产物；没有移动旧tag、修改现有数据库/secret/权限、安装、pull运行镜像或部署。后续仅任务归档/日志提交，不能改变已获CI/Release认证的产品代码。
