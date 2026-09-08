# 发布 Aether v0.7.31

## Goal

发布新的稳定应用 tag `v0.7.31`，使用已获精确 SHA CI 成功的 `74abb40cfd8f4c9df93ce657c8bbcbf9996acda6`，验证自动 Release 工作流、发布资产和 GHCR 多架构镜像；不部署。

## Requirements

1. 最新正式应用 Release 为 v0.7.30，本次按补丁号递增；不修改独立 Tunnel tag。
2. tag 使用中文说明的 annotated tag，目标必须是 Rust CI34220941447 已18/18成功的74abb40cf。当前master4f7e32c2c与其仅.trellis记录/说明不同，产品源码完全一致。
3. 只推送新tag，不重跑已通过的完整CI、不修改产品代码，不覆盖/删除已存在tag或Release。
4. 跟进由tag push触发的 Release Aether 到实际终态，验证版本、目标SHA、非空产物、SHA256SUMS与资产digest、linux/amd64及linux/arm64镜像。
5. 发布范围包含当前工作流规定的VSIX和provenance产物。失败时如实记录，不把“tag已推送”当作“发布成功”；不得悄悄移动已发布tag。
6. 不部署、不修改现有数据库、权限/secret或付费基础设施。

## Acceptance Criteria

- [x] 核对最新tag、目标产品源码一致性与精确SHA CI成功。
- [x] 本地/远端v0.7.31 annotated tag均指向74abb40cf。
- [x] Release工作流成功，Release页面和所需资产非空。
- [x] 资产checksum/digest及GHCR多架构manifest核验通过，明确实际验证边界。
- [x] 发布证据已记录，按finish-work清理任务自有临时文件并归档，不改部署。

## Plan and Boundary

这是既有发布流程的执行任务，不改产品实现。Root负责tag/Git/任务状态与发布结果；监测可用只读代理。步骤为创建并推新tag、跟进Release、核验产物、归档。

当前Release工作流新增VSIX构建，实际应按当前展开job和产物统计，不复用旧版本7-job数量。若需修源代码或权限才能发布，先报告证据与可行下一步，不重打现有tag。

## Live Release Evidence

- tag对象：`188ace217a49886f7542b3d5aaf88faac8d0e1a9`；本地和远端解引用均为 `74abb40cfd8f4c9df93ce657c8bbcbf9996acda6`。
- Release run：`34230430079`，https://github.com/ZipperCode/Aether/actions/runs/34230430079，UTC 2026-09-08 13:11:14由push触发。
- Release全部8/8成功，已公开发布为latest稳定版本；17m52s完成。
- 预期资产：两架构tar.gz、install.sh、SHA256SUMS、AETHER_RELEASE_PROVENANCE.sigstore.json、aether-vscodex-0.4.0.vsix；镜像ghcr.io/zippercode/aether:0.7.31。
- 实际6资产和双架构镜像已核验；小资产本地hash、两tar服务端digest/SHA256SUMS、来源签名验证通过。完整数据和未运行安装/部署边界见release-verification.md。
