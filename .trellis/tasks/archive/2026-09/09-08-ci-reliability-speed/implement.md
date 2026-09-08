# 执行计划

- [x] 核对本地/远端 SHA，保存真实失败、运行耗时和当前工作流证据。
- [x] 选择有证据的最小实现，保留所有原有门禁与功能，完成 PRD/design。
- [x] build 叶子修复 Git watch 和最小 Cargo fixture；pipeline 叶子统一检查入口、CI 命令/环境/trigger。
- [x] 串行整合两组接口，运行两份快速回归、配置解析与受影响源合同检查。
- [x] 独占 Trellis check 验证无覆盖减少、无外部操作、无不必要抽象，并自修复实际问题。
- [x] Root 更新项目 CI 契约；本地代码与证据已完成，按 finish-work 归档，不 push/dispatch/部署。

## 验证命令与范围

- `python tests/gateway_build_watch_test.py`：只构建无依赖临时包，验证不变输入不重跑、HEAD/branch/版本环境变化正常重跑；不编译真实网关。
- `python tests/ci_contract_test.py`：验证共享命令与 CI 调用、参数/env、dry-run与失败传播，避免测试逻辑仅重复源码字符串。
- `python tools/ci.py preflight --dry-run`：只打印即将使用的真实步骤；不将 dry-run 当编译通过。
- YAML 解析、`rustfmt --check` 仅 build.rs、`git diff --check`、已有相关发布供应链静态 fixture。
- 如改动只涉及上述配置/脚本及 build.rs，不跑完整 Rust/前端测试；远端速度和首次通过率未实测，最终明确标注。

## 写集

- build writer：`apps/aether-gateway/build.rs`、`tests/gateway_build_watch_test.py`、本任务 `research/build-implementation.md`。
- pipeline writer：`.github/workflows/rust-ci.yml`、`tools/ci.py`、`tests/ci_contract_test.py`、`Makefile`、`apps/aether-gateway/AGENTS.md`、本任务 `research/pipeline-implementation.md`。
- Root：PRD/design/implement、JSONL、spec及 Git/任务生命周期；派发期间不读取/重复产品调查，不与叶子写相同文件。
- check：所有叶子停写后才取得上述产品写集；新增路径须明确交接。

## 当前状态

2026-09-08：两组实现已终态并归还非重叠写集。旧build脚本已在linked unchanged场景真实复现缺失watch路径，修复后23次最小Cargo检查通过；共享命令fixture、dry-run、YAML基线17job比较和供应链静态检查通过。Root已检查实际diff及收据并更新CI/构建契约，下一步独占check跑组合快速验证；不运行全网关/前端/真实DB/远端CI，无存活任务进程。

最终独占检查追加 packed-only Fresh 覆盖后，共24次Cargo检查通过（21.96秒）；CI入口回归、完整YAML/基线门禁对比、rustfmt、diff和供应链fixture均通过。可选的隔离Clippy探测被策略拒绝且未执行，不影响已确认最小验收；未绕过。没有全仓重编译或实际GitHub运行，Root准备本地提交与任务归档。
