# 发布 Aether v0.7.39

## 目标和授权

按用户要求推送已完成的 Codex OAuth 修复，等待完全相同提交的 GitHub Rust CI 全部成功后，创建并推送 annotated tag v0.7.39，完成 tag 触发的发布和资产验证。发布不包含生产部署。

## 发布候选

- 初始候选：22124fe5984ea1fb52c8d6b3195b999d9a1c6fbb，master 已推送且远端 ref 核对一致。
- 核心修复：b2b87ce1547e88b6b6ea9a61a5cacfba628d9597。
- GitHub Rust CI：36091500070（push）；必须验证其 headSha、所有 job 和 conclusion。
- 最新已发布版本 v0.7.38；已核对目标 tag 不存在。

## 验收与步骤

- [x] 已提交任务代码并推送 master，远端 SHA 一致。
- [x] 精确候选完整 CI 成功：36091500070，18/18 jobs success。
- [x] 已创建并推送 annotated v0.7.39；tag object 934f4534d76a3489e1b3fe8f32ccf9172befa2c0，远端 peeled SHA 等于 CI 候选。
- [x] 同 tag/SHA 的 Release Aether 36092942236 全部成功，8/8 jobs；6 个资产本地哈希匹配，两个 tarball 的 SHA256SUMS 与包结构通过，4 个签名 subject 及 GHCR exact-source provenance 通过。
- [x] 保存验证记录并完成独立复核；归档和会话记录随发布收尾提交，已发布 tag 保持不变。

## 边界

不部署154、不更改生产数据库/Redis/凭据、不恢复 GitHub Pages、不强推。CI 若揭示本次变更缺陷，可在已授权范围修复并重走 exact-SHA CI。

## 发布运行

Release Aether 36092942236，tag=v0.7.39，headSha=22124fe5984ea1fb52c8d6b3195b999d9a1c6fbb，已成功。证据见 `ci-receipt.json`、`release-run.json`、`release-receipt.json`。

- 发布：https://github.com/ZipperCode/Aether/releases/tag/v0.7.39
- GHCR：`ghcr.io/zippercode/aether:0.7.39`，digest `sha256:a0f96036601ca36bcb05d4a4529dcacf22eae85a633f14f773ae0261a317205d`，amd64/arm64 均已核验。
- VSIX 保持其独立扩展版本 0.4.0；校验 GitHub digest 与 ZIP 完整性，不将其宣称为 tarball provenance 的签名 subject。
- 未部署，未进行生产写入；本次下载校验临时文件已清理。
- 独立审核确认 CI 于 04:03:15 UTC 完成，annotated tag 于 04:04:11 UTC 创建；满足先 CI 后 tag。远端 refs、6 个资产元数据、Release 和 Pages 停用状态均复核通过。
