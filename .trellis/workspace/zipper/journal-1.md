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


## Session 5: 修复特殊模型 Endpoint 推断与公开目录

**Date**: 2026-08-21
**Task**: 修复特殊模型 Endpoint 推断与公开目录
**Package**: aether-gateway

### Summary

统一模型能力与 API 格式族语义，修复图像模型 Endpoint 自动推断，并让标准 OpenAI 模型列表和详情发布 Key 可见的全部 Global Models。

### Main Changes

- Global Model image_generation 能力自动匹配 openai:image Endpoint
- 标准 OpenAI /v1/models 与详情跨模型族发布，同时保留 allowed_models 和 Provider 限制
- Claude、Gemini、Codex 目录保持原有协议过滤

### Git Commits

| Hash | Message |
|------|---------|
| `9a05f2b027bd3465f61a9be7713234423952c367` | (see git log) |

### Testing

- [OK] 目标 Rust 文件 rustfmt --check 通过
- [OK] git diff --check、CodeGraph 影响检查与两轮静态审查通过
- [OK] 按项目约束未运行大模块编译或单元测试

### Status

[OK] **Completed**

### Next Steps

- 在运行环境关联 gpt-image-2 并调用 /v1/models 做端到端确认


## Session 6: 同步上游 main 分支

**Date**: 2026-08-23
**Task**: 同步上游 main 分支
**Package**: aether-tunnel
**Branch**: `master`

### Summary

合并 origin/master 与 fawney19/Aether main，解决 52 个冲突并保留本地 Endpoint、Responses 错误与余额调度契约；Rust workspace 和前端类型检查通过，已回并 master 并清理隔离 worktree。

### Git Commits

| Hash | Message |
|------|---------|
| `821de21b3` | (see git log) |
| `e4e223cb8` | (see git log) |
| `41636cd19` | (see git log) |

### Status

[OK] **Completed**


## Session 7: 按模型筛选可排序 Provider

**Date**: 2026-08-24
**Task**: 按模型筛选可排序 Provider
**Package**: aether-tunnel
**Branch**: `master`

### Summary

按当前全局模型、Provider 启用状态和启用 Key 筛选模型级 Provider 排序，保留隐藏覆盖值并完成 Docker 预览与调度影响审查。

### Git Commits

| Hash | Message |
|------|---------|
| `a0aa52967` | (see git log) |

### Status

[OK] **Completed**


## Session 8: 发布 Aether v0.7.25

**Date**: 2026-08-24
**Task**: 发布 Aether v0.7.25
**Package**: aether-tunnel
**Branch**: `master`

### Summary

补齐前端 CI 门禁，逐轮修复 Gateway Clippy、流取消测试、响应体上限兼容与候选队列陈旧断言；从 23/23 Rust CI 全绿 SHA 发布 annotated v0.7.25，并验证 7/7 Release jobs 与四个资产。

### Git Commits

| Hash | Message |
|------|---------|
| `7a42452f2bb9d9a34cdd9a8263df2365c4cc5eaf` | (see git log) |
| `3ab6787a9e33746387e849174f9c93b632d4deac` | (see git log) |
| `beb1ef6b78f4c82f65301c3ead4dd61b3a248477` | (see git log) |
| `b31b8ebb41dfaaaf7f60872c4fa89ed9aaff3478` | (see git log) |
| `ec32c696d9b93e63de5c1333cddb34c6bcc7df9e` | (see git log) |
| `92def9b5012f0e5c2db148aa45310de675d51bf5` | (see git log) |
| `1ed38e84518366d8a547c2e2a27c3496b8de3cd8` | (see git log) |

### Status

[OK] **Completed**


## Session 9: 优化同格式请求与流式热路径

**Date**: 2026-08-28
**Task**: 优化同格式请求与流式热路径
**Package**: aether-tunnel
**Branch**: `master`

### Summary

在同格式且请求体未变化、无编码压缩语义时复用前门规范化字节；移除传输 JSON 克隆与无转换流分片复制，并补齐契约和回归测试。

### Git Commits

| Hash | Message |
|------|---------|
| `37e42296c` | (see git log) |

### Status

[OK] **Completed**


## Session 10: 模型能力检测与 v0.7.26 发布准备

**Date**: 2026-08-30
**Task**: 模型能力检测与 v0.7.26 发布准备
**Package**: aether-tunnel
**Branch**: `master`

### Summary

完成模型能力检测跨层实现、失效参考 fail-closed 修复与全范围验证，准备推送并发布 v0.7.26。

### Git Commits

| Hash | Message |
|------|---------|
| `6d537bae3` | (see git log) |

### Status

[OK] **Completed**


## Session 11: 同步上游路由、Provider Pool 与协议兼容功能

**Date**: 2026-09-03
**Task**: 同步上游路由、Provider Pool 与协议兼容功能
**Package**: aether-tunnel
**Branch**: `master`

### Summary

同步用户选定的上游第 1、3、4、5 组能力：路由策略与按需粘性重试、Key 并发和缓存亲和及模型额度、Codex/网关流式兼容、Gemini/Responses 协议修复；保留 fork 合同，排除 VSCodex、套餐撤销、通用 Usage API 与 Nightly，完成验证、fast-forward 合并和临时 worktree 清理。

### Git Commits

| Hash | Message |
|------|---------|
| `f485ae016` | (see git log) |
| `09c4aab9e` | (see git log) |
| `02ac8aac0` | (see git log) |
| `72994512b` | (see git log) |
| `9a3f17acd` | (see git log) |
| `192782763` | (see git log) |

### Status

[OK] **Completed**


## Session 12: 发布 v0.7.27 并修复同步后 CI

**Date**: 2026-09-03
**Task**: 发布 v0.7.27 并修复同步后 CI
**Package**: aether-tunnel
**Branch**: `master`

### Summary

补齐强准入后的流式、同步、心跳和视频测试夹具；精确 SHA 的 Rust CI 全绿，发布 v0.7.27 并核验多架构资产与校验和。

### Git Commits

| Hash | Message |
|------|---------|
| `d0ef009cc` | (see git log) |
| `debf52535` | (see git log) |
| `d24695454` | (see git log) |
| `fa4d32da2` | (see git log) |
| `94ddca675` | (see git log) |

### Status

[OK] **Completed**
