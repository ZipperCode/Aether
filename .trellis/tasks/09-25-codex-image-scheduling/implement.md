# 实施计划

1. 完成原始路径与外部实现对照（已完成）。
2. 三个互不重叠切片同时实施：模型发现共享补全及自动刷新；管理发现结果统一补全；同步/流式生图调度回归。共享接口见 design.md 与 implement.jsonl，agent 禁止中途 build/lint/tests/formatter。
3. 用户报告的调度失败作为既有复现事实；新增回归明确覆盖原本缺失的模型发现白名单与真实图片请求路径。使用 cargo test（nextest 未安装，不安装）统一验证。
4. 运行 aether-model-fetch 测试、gateway 模型发现/管理/图片链路的相关回归，并用真实本机 HTTP 请求验证发现恢复到图片执行；检查 LSP diagnostics 和格式。
5. 验证后更新现有 API 文档与 model-fetch 契约，说明配置、旧白名单刷新步骤、真实账户权限未验证边界。不得提交 Git 或修改已有他人变更。

## 当前验证记录

- `cargo test --offline -p aether-model-fetch -p aether-scheduler-core --lib`：189 项通过。
- gateway `codex` 范围：最终 251 项通过；gateway + ai-formats `image` 范围：288 项通过，含真实同步/流式 HTTP 生图与原生图片事件保真；`direct_execution_frame_stream`：8 项通过，包含伪造桥接标记剥除。
- 独立临时 Rust 可执行程序调用已编译补全与过滤函数：输入文本 ID `gpt-5.4-mini`，输出白名单 `[gpt-5.4-mini, gpt-image-2]`；exclude `gpt-image-*` 后仅保留文本 ID，Endpoint 为 `image-endpoint`。程序退出 0，临时产物已删除。
- 本次 11 个 Rust 文件限定范围 rustfmt 检查通过，保留 CRLF。
- Windows 默认测试线程栈在既有 quota 测试溢出；后续使用 `RUST_MIN_STACK=16777216` 与 `--test-threads=2`。未改该测试或生产代码。
- rust-analyzer component 未安装，LSP diagnostics 无法运行；不安装工具。未使用真实 Codex 账户或 PostgreSQL，不声称验证这些环境。
- 最初真实流式回归确实失败（HTTP 200 空 SSE）；修复帧泵预桥接与 relay 重复转换后同一测试通过。内部转换 marker 只由实际桥接产生、剥除上游伪造值、不向客户端泄露；本地桥接已发送状态与帧泵已转换状态分离。
- 原生 `image_generation.*` / `image_edit.*` SSE 保留原块、多图片 completed 与未知扩展；原有 Responses→Images 转换保持。
- 无 Git 写操作、无依赖安装、无部署与外部账户调用；保留任务前四个已有修改。任务工件留在当前目录，不调用会自动提交的归档脚本。
- `sse_body_stream`：10 项通过，验证 keepalive、分片与最终 HTTP 字节；最终格式化后的 `image` 范围再次 288 项通过。一次错误模块名 filter 执行 0 项，不计为验证，已用真实函数名 filter 完成检查。临时 IMGDIAG/DIAG 与重复模型常量残留检查为零。
