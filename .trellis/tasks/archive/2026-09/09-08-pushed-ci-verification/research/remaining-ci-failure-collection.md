# CI 剩余失败收集修复回执

- 状态：READY，2026-09-08；本组停止写入，交还 Root 整合。
- 基线：`948c1c16f2f9927b37a8570b86767128abd6b7c8`，Rust CI run `34210248162`。
- 本次独占范围：`.github/workflows/rust-ci.yml`、`tests/ci_contract_test.py` 和本回执；未重新编辑已交回的 frontend 文件或其他所有者文件。

## 触发证据与改动

Root/日志读取代理确认本轮两个 job 被 nextest 首个失败提前中止：

- Test (Data)，job `102009266095`：97 passed、1 failed，274 个测试未运行。
- Test (Workspace Rest)，job `102009266040`：1594 passed、1 failed、17 skipped，1948 个测试未运行。

仅将两个实际命令追加 `--no-fail-fast`：

```diff
- cargo nextest run -p aether-data
+ cargo nextest run -p aether-data --no-fail-fast
- cargo nextest run --workspace --exclude aether-gateway --exclude aether-data --exclude aether-integration-tests
+ cargo nextest run --workspace --exclude aether-gateway --exclude aether-data --exclude aether-integration-tests --no-fail-fast
```

两个命令各附中文注释，说明收集全部失败而非忽略失败。保留测试目标、排除范围、环境、PostgreSQL 必跑要求及失败退出；没有增加 `--locked`、过滤条件或其他参数。成功运行的覆盖不变；有失败时可能耗时更长，以便下一次推送尽量暴露剩余失败，不宣称一定一轮完成。

现有 `ci_contract_test.py` 增加一个 9 行范围化块，精确核对两条命令及禁止 `continue-on-error`；不新增框架、脚本或 YAML 依赖。该脚本原有完整 job 集合、依赖及非成功结果导致 `exit 1` 的断言仍保留。

## 验证

- 先仅添加契约断言、未改工作流时运行 `python tests/ci_contract_test.py`：exit 1，准确指向 `test_data` 仍为缺少标志的旧命令。
- 工作流改完后同一命令：PASS，exit 0；执行真实 CI dispatcher 参数/env/dry-run/失败传播检查，但 Cargo 子进程由既有 mock 替换，没有编译网关。
- 已安装 PyYAML 6.0.3 `BaseLoader` 解析完整当前工作流及 `git show 948c1c16f...:.github/workflows/rust-ci.yml` 基线。仅在基线对象的两个 nextest `run` 值追加标志后，两份完整对象严格相等：PASS，exit 0。
  - **17 个 job 定义、矩阵展开 18 个 job** 均保留。
  - 其余所有字段完全不变：顶层触发/并发/权限/env、所有 job ID、步骤、依赖/if、汇总 gate、action pins/cache/profile、feature/adapter/gateway/integration/DB 检查均未变化。
  - 额外核对 `test_data` 中 `AETHER_REQUIRE_LOCAL_POSTGRES_TESTS == "true"`：PASS。
  - 初次对比因 PowerShell 收集 `git show` 丢失最终换行，导致末尾 YAML 块标量差异；对两份输入统一末尾换行后严格对比通过。没有为通过检查修改工作流额外字段。
- `git diff --check -- .github/workflows/rust-ci.yml tests/ci_contract_test.py`：PASS，exit 0，仅 Git 既有 LF/CRLF 转换提示。

## 边界、风险与清理

- 未运行 Cargo、前端、数据库或 GitHub 操作；未 commit/push、取消/重跑 CI、变更权限/付费 runner/依赖，也未更新任务公共状态或 specs。
- 没有新增临时文件、日志、调试开关或后台进程；所有检查自然退出。
- 这只是命令与配置验证，不是完整 CI PASS。Root 仍须批量推送本轮实际修复后，核验精确新 SHA 的完整运行与剩余失败。
