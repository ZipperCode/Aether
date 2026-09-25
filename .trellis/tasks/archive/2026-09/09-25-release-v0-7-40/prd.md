# 发布 Aether v0.7.40

## 目标与授权

继续用户既有“提交代码，push GitHub，CI 编译通过之后推送新的 tag”要求，发布经验证的 Codex Images 输出格式与全候选跳过漏记账修复。发布与154部署分开。

## 发布前核查

- 当前 origin 为 `git@github.com:ZipperCode/Aether.git`，分支 master。
- 初始本地与远端基线均为 `8e765877e69a17f98c928003b5740bd7d6b293b1`。
- 2026-09-25 核对最新发布为 v0.7.39；远端 v0.7.40 尚不存在，推 tag 前再核对。
- 修复任务已归档：`.trellis/tasks/archive/2026-09/09-25-codex-body-build-replay/`。
- 修复提交：`c601d38e9e0748d835b7f9df43a4e624c4acdc05`；归档和会话记录后，发布候选为 `ec4d8d1da8b8a23e6074c4195e8dee4965f8cb14`。
- master 已推送且远端SHA一致；Rust CI push运行 `36101864221`，创建于 `2026-09-25T06:12:55Z`，18/18成功。

## 步骤与验收

- [x] 修复通过本地120项相关测试、Clippy、格式检查及独立代码复核，提交代码与任务记录。
- [x] 推送 master 并锁定远端完全相同的候选 SHA。
- [x] exact-SHA Rust CI `36101864221` 于 `2026-09-25T06:33:03Z` 全部18项成功，证据见 ci-receipt.json。
- [x] CI完成后于 `2026-09-25T14:33:48+08:00` 创建并推送 annotated v0.7.40；tag object `b55458562810f78c3a30677f6053bd978ff59907`，远端 peeled SHA与候选相同。
- [x] tag对应 Release Aether `36103449902` 全部8项成功；6个资产本地哈希匹配，两个tarball各256项且安装器一致，4个签名subject及GHCR exact-source provenance通过。
- [x] 完成只读发布复核；发布证据随收尾归档提交。

## 边界

不部署154、不修改线上数据库/Redis/凭据，不恢复 GitHub Pages、不强推。
若 CI 揭示本次修复问题，修复后重新推送并等待新 SHA 的完整 CI；未认证的提交不能打 tag。

代码独立审查已在修复任务完成；复核代理随后触及额度，发布复核由主任务通过 GitHub运行元数据、资产本地哈希和签名工具完成。

## 发布结果

- Release：https://github.com/ZipperCode/Aether/releases/tag/v0.7.40
- 镜像：`ghcr.io/zippercode/aether:0.7.40`。
- Digest：`sha256:600132c8604ed497c82a68e99b0e3bfa972ec50f6457ac4fce7bf6147a684ad8`，linux/amd64和linux/arm64均已验证。
- CI、Release运行与完整资产验证分别见 ci-receipt.json、release-run.json、release-receipt.json。
- VSIX保持独立版本0.4.0，校验本地哈希及ZIP完整性，不将其标记为工作流已签名subject。
- Pages API及公网站点仍404，frontend base保持 `/`；未部署154，也未操作生产配置/数据库/Redis。
