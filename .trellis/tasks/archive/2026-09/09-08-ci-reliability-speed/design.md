# CI 可靠性与耗时优化设计

## 证据与目标

- 历史三次成功 Rust CI 为 16m42s–18m20s；Gateway 是关键路径，lib 构建 6m07s–7m26s、测试约 397–400s，bins 另构建约 3m10s–3m20s、测试约 1s。不能宣称合并命令就省去整个 bins 构建。
- 五个失败样本均在 Gateway lib 行为测试，源于合同/夹具漂移；nextest 默认快速停止使后续遗漏分轮暴露。样本不是失败率统计。
- 本地已复现 build.rs 在 linked worktree 监听不存在的 `.git/HEAD`，导致未改源码也反复编译；这是本地确定性缺陷，不是普通 GitHub checkout 耗时的直接归因。
- 现有 target cache 已命中，sccache/mold/cancel-in-progress 已存在。缓存 action 使用存在的附注 tag 对象，不把它当坏 pin；本次不替换 pin、不加新缓存基础设施。

## 最小改动

### A. 构建版本监听

- 只改 `apps/aether-gateway/build.rs` 的 Git 监听路径拥有者，使用 Git 自己的路径解析支持普通 checkout、linked worktree 和 detached HEAD；只监听实际存在的必要路径。
- 没有 Git 的源码包仍监听 `build.rs` 和现有显式版本环境输入，保留所有版本优先级、`git describe` 与 tunnel tag 排除语义。
- 不递归监听整个 `.git`，不为了脏状态/任意 tag 变化引入新的大范围重编译策略；不新增依赖或通用 Git 抽象。
- 用 stdlib Python + 临时极小 Cargo 包验证实际 build-script 重跑行为，不编译整个网关；测试由 `tests/gateway_build_watch_test.py` 持有。

### B. 网关 CI 命令单一来源

- 新增一个 stdlib Python 入口 `tools/ci.py`，只覆盖 `fmt`、`clippy-gateway`、`gateway` 及组合 `preflight`；提供 `--dry-run` 展示命令，不伪装完整工作区/数据库/前端 CI。
- Gateway 命令统一为 `cargo nextest run -p aether-gateway --lib --bins --no-fail-fast --locked`。运行同样目标，并一次收集所有行为失败，返回失败码而非自动重试或吞错。
- 统一已有 CI 的 incremental/debug 设置以及 Gateway 16 MiB 测试栈；不强制本地使用 Linux linker，不自动安装工具、不读取 `.env`、不启动服务。
- Makefile 只做薄入口，CI 对应 fmt、Clippy Gateway、Gateway steps 调用同一 Python 源；其他 CI 命令/feature/数据库与汇总 gate 不动。
- 快速 fixture 只验证命令、环境、失败传播和工作流调用合同，不新增泛用 CI 框架。

### C. 工作流对齐

- Gateway 的 mold `RUSTFLAGS` 上移至 job env，使缓存恢复阶段可以看到与实际编译一致的 flags；保留 sccache 的现有执行环境，不改变缓存容量、存储或权限。
- 全部 Rust setup 显式使用现有 1.95.0，避免安装不用的 moving stable；不声称历史 Cargo 实际使用了错误编译器。
- 补 `rust-toolchain.toml`、`aether-vscodex/**`、共享入口及回归脚本的 push/PR path filters。
- 将两个快速脚本回归放入现有格式检查 job；不新增昂贵阶段依赖，不增加正常关键路径串行等待。

## 保留与暂缓

- 不删功能、测试、feature matrix、数据库门禁或最终 check 条件；不改 exact-SHA CI-before-tag 发布流程。
- 本轮不做 gateway 分包、nextest archive/sharding、付费 runner、workspace-crate 缓存策略、测试超时调大或忽略 flaky。
- 记录 adapter/nightly/VSCodex 重复工作但本次不扩至非关键路径；主目标是网关失败反馈和重复构建，不制造没有实测的百分比承诺。
- Windows 本地与 Linux CI 仍有 OS/linker/数据库差别，入口对齐命令不等于完整环境同构；文档须明确需已有工具链和 nextest。

## 所有权与验证

- build 叶子独占 build.rs 和 `tests/gateway_build_watch_test.py`；pipeline 叶子独占 workflow、`tools/ci.py`、`tests/ci_contract_test.py`、Makefile 和 gateway AGENTS 的命令说明；两组非重叠。
- 两组结束后独占 check 审查需求与必要脚本测试。Root 持有任务工件/规范及最终提交，研究与实现证据保存在本任务 research。
- 使用临时最小包、命令 dry-run/mock、配置解析和源合同检查；不重复前一任务的全工作区构建/UI测试。GitHub实际时长必须在后续获准推送运行后才可测量。
