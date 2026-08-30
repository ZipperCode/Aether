# 实施计划

1. 后端合同与纯逻辑
   - 定义 typed 请求/响应、枚举、参考映射校验。
   - 实现 `capability-v1` 题目生成、答案解析、Wilson 区间、精确 McNemar 和 verdict。
   - 添加一组集中测试覆盖可复现性、配额、解析和阈值。
2. 后端执行与路由
   - 复用 provider-query 模型测试候选、模型映射、adapter 和 execution runtime，固定单候选执行。
   - 添加并发/总时限/取消、失败分类、结果脱敏和 provider-query 权限路由。
   - 用 mock 上游覆盖主要文本格式、相同题集和无 failover。
3. 前端
   - 增加 API/types、模型行能力检测入口和独立对话框。
   - 复用现有 provider/model/endpoint/Key API；合并保存 `capability_test_reference`。
   - 实现配置、运行、结果与 100 题复核状态，保留其他模型 config。
4. 集成检查
   - 对照 PRD 检查跨层字段、错误枚举、取消与脱敏。
   - 修复检查代理发现的问题，不扩大到历史、调度或外部 runner。
5. 规范与收尾
   - 将稳定的模型能力检测合同写入 gateway backend spec 并更新索引。
   - 更新任务进度、运行范围化验证、提交中文 commit，归档任务。

## Validation

```text
cargo test -p aether-gateway capability --lib
cargo fmt --all --check
cd frontend && npm run type-check
git diff --check
```

若新前端纯状态 mapper 增加独立测试文件，只运行该文件；不运行会改文件的 `npm run lint`，不做视觉验证，不默认跑全 workspace。

## Risk and Rollback Points

- 候选固定与禁止 failover 是正确性边界；发现复用 helper 会改变现有联通测试时，拆出只读解析 helper 而不修改旧语义。
- 多协议答案抽取必须复用现有聚合器；不能为能力检测发明第二套 SSE/JSON 解析。
- 模型 config 更新必须合并旧值；任何覆盖风险先停止前端写入并修正。
- 若同步请求在范围测试中无法可靠取消或达到硬时限，再回到设计阶段；不临时创建通用后台任务框架。

## Completion Evidence

- `RUST_MIN_STACK=16777216 cargo test -p aether-gateway capability --lib`: 27 passed, 0 failed.
- OpenAI Responses、Claude Messages、Gemini GenerateContent 聚合精确测试：各 1 passed, 0 failed.
- `cargo clippy -p aether-gateway --lib --bins --examples -- -D warnings`: passed.
- `cargo fmt --all --check`: passed.
- `cd frontend && npm run type-check`: passed.
- `cd frontend && npx vitest run src/features/providers/components/provider-tabs/__tests__/model-test-request.spec.ts`: 70 passed, 0 failed.
- `git diff --check`: passed；仅有 Windows LF/CRLF 提示。
- 未运行真实付费 Provider 或视觉验证；该项按任务验证边界保留为风险，不影响本地逻辑与 mock 合同验收。
- `docker build -f Dockerfile.app.local -t aether-app:model-capability-local .`: 当前工作区 release 镜像构建成功；首次 Docker Desktop 崩溃导致 BuildKit EOF，重启后复用缓存完成，最终 Rust release 链接耗时 13m20s。
- Docker 预览使用旧 Postgres volume 的任务专用离线克隆，并仅在克隆上执行迁移/回填；原 `aether_postgres_data` 与旧 `aether-app` 保持未修改、未删除。
- 运行证据：`aether-model-capability-local` healthy，`/_gateway/health` 与 `/admin/providers` 均 HTTP 200，生产 API asset 含 `test-model-capability`，Provider drawer asset 含身份认证边界文案。
- 用户验收修复：能力检测弹窗增加右上角无障碍关闭按钮，并恢复遮罩与 `Esc` 关闭；三个入口均复用 `handleDialogUpdate(false)`，先取消运行中的请求再关闭。
- 修复验证：`cd frontend && npm run type-check` 通过，Trellis 定向检查 PASS；重建镜像 `sha256:e967ef9de7c01e3a51ff879f3d53423a36c508905e3dd9e4ea619b317fea08fc` 后容器 healthy，健康页、Provider 页和新 Provider drawer asset 均 HTTP 200，生产资源含关闭按钮的无障碍文案。
