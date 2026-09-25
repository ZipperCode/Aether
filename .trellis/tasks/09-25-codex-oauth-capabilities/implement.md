# 执行计划

- [x] 检查本地基线、线上只读证据和现有 Codex 模板五种 Endpoint。
- [x] 核对 sub2api 和官方套餐证据；完成 PRD 与设计，明确本地实施授权和生产边界。
- [x] Discovery worker：动态图片配置补全、Compact/Search 投影、新鲜与旧缓存/自动同步一致性、相关回归；已交付检查。
- [x] Runtime worker：Free 图片共享准入、管理测试空白名单一致性、诊断映射、Search 最终请求头和真实本机协议回归；已交付检查。
- [x] 主会话集成两个工作流，检查每个模板 Endpoint 和每个用户要求。
- [x] Trellis check worker 已完成全范围审查并修复；主会话统一执行最终针对性测试，避免多个 Cargo 争用同一 target。
- [ ] 更新规格、任务结果和会话记录，中文提交本地，不推送或部署。

## 验证命令

- `cargo fmt --all --check`、`git diff --check`。
- `cargo test --offline -p aether-model-fetch --lib`。
- `cargo test --offline -p aether-provider-transport --lib codex`（根据新用例名补充 capability/transport policy 过滤）。
- `RUST_MIN_STACK=16777216 cargo test --offline -p aether-gateway --lib <相关过滤> -- --test-threads=2`：发现/管理查询、Search、Image、Compact、Live、相关架构依赖边界；只执行受影响回归。
- 若前端诊断映射变更：`npm run type-check` 与对应 vitest 文件；不运行带 `--fix` 的 npm lint。

## 进度与风险

未验证真实上游账号权限/成功生成；本机 HTTP 测试只证明网关协议与调度契约。未知套餐和没有原生能力字段的旧账号不额外猜测权限。

## 已执行验证

- `cargo test --offline -p aether-model-fetch --lib -- --test-threads=2`：96 项通过。
- `cargo test --offline -p aether-provider-transport --lib codex -- --test-threads=2`：32 项通过。
- `cargo test --offline -p aether-ai-formats -p aether-provider-pool --lib codex -- --test-threads=2`：86 + 11 项通过。
- 前端 `npm run type-check`：退出码 0。
- 前端定向 `skipReason.spec.ts` + `model-test-request.spec.ts`：76 项通过。
- Gateway 首轮 `cargo test --offline -p aether-gateway --lib codex -- --test-threads=2`：255 项通过（包括本机真实 Images/Search HTTP、Live calls/sideband/WS 和相关 OAuth/额度回归）。审查最后的映射/预设修正由后续稳定构建覆盖。
- 全仓 `cargo fmt --all --check` 与 `git diff --check` 通过。
- 审查修正后的稳定 Gateway 构建：`model_fetch::` 65 项、`codex` 255 项、`compact` 34 项、原有 OpenAI 管理发现夹具 1 项均通过。
- 最后依赖边界检查发现预设路径直接使用格式 crate，已改为同一函数的 `crate::ai_serving` 入口；对应检查通过。使用该稳定编译测试程序补跑完整 `tests::architecture::ai_serving::`：61 项通过。
- 最终 model-fetch 96 项、Search 格式 3 项、memory/PostgreSQL 轻量投影各 1 项、诊断持久化 1 项、套餐来源解析 2 项通过；数据验证是单元/SQL 投影检查，不是生产数据库写入或真实 PostgreSQL smoke。
- 受影响 8 个 Rust 包 `cargo clippy --offline ... --lib --bins --examples -- -D warnings`：退出码 0；2026-09-25 继续收尾时重新确认该结果。

## 最终审查修正

- 修复两个新增测试夹具的引用/Arc 类型编译问题。
- 强制刷新已投影的兼容目录不再被误认成原生卡片。
- Search 最终 Content-Type 同样固定 JSON，真实 HTTP 用例覆盖认证覆盖头和 Endpoint 规则污染。
- 管理测试与用量视图都显示清晰 Free 拒绝文案；对应前端 76 项回归和 type-check 已重跑通过，4 个改动 TS 文件的只读 ESLint 通过。
- 图片映射的空 API/Endpoint 作用域沿用正式候选拒绝；operations 空列表仍按既有规范化继承。
- 仅有图片 Endpoint、套餐仅存于 OAuth auth 的预设路径，也通过一次受限选中 transport 读取识别 Free；无上游调用。
