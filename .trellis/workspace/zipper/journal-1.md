# Journal - zipper (Part 1)

> AI development session journal
> Started: 2026-08-19

---



## Session 1: 完成余额感知 Key 调度与调度审查

**Date**: 2026-08-19
**Task**: 完成余额感知 Key 调度与调度审查
**Package**: aether-tunnel
**Branch**: `master`

### Summary

实现低余额 Key 自动跳过与恢复，统一普通、Pool 与 sticky 过滤，补齐自动自检和缓存失效，并完成核心及 Gateway 定向验证。

### Git Commits

| Hash | Message |
|------|---------|
| `3d277c56d` | (see git log) |
| `0b0c6d3b4` | (see git log) |

### Status

[OK] **Completed**


## Session 2: 完善 Codex HTTP Responses 中转契约

**Date**: 2026-08-19
**Task**: 完善 Codex HTTP Responses 中转契约
**Package**: aether-tunnel
**Branch**: `master`

### Summary

核对 OpenAI Codex HTTP 客户端后，将范围收敛为 Responses create、compact 与 HTTP SSE；补齐同格式字段、header、compact、opaque SSE、错误和 failover 回归，并记录无持久化的跨包契约。

### Git Commits

| Hash | Message |
|------|---------|
| `793ac8625` | (see git log) |
| `0a26f87d9` | (see git log) |

### Status

[OK] **Completed**


## Session 3: 发布 Aether v0.7.24

**Date**: 2026-08-19
**Task**: 发布 Aether v0.7.24
**Package**: aether-tunnel
**Branch**: `master`

### Summary

普通推送锁定 SHA，Rust CI 22/22 全绿后创建 annotated tag v0.7.24；Release Aether 7/7 成功并核验四个发布资产。

### Git Commits

| Hash | Message |
|------|---------|
| `d215535ac` | (see git log) |

### Status

[OK] **Completed**


## Session 4: 修复 Responses 流式错误提交

**Date**: 2026-08-20
**Task**: 修复 Responses 流式错误提交
**Package**: aether-tunnel
**Branch**: `master`

### Summary

修复同格式 OpenAI Responses SSE 首段裸错误在 HTTP 200 提交后导致客户端缺少 id 的反序列化问题；复用首段分类与候选重试路径，并补充回归测试和协议规范。

### Git Commits

| Hash | Message |
|------|---------|
| `f86e9941a` | (see git log) |

### Status

[OK] **Completed**
