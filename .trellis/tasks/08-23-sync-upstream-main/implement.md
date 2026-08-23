# 执行计划

- [ ] 记录任务分支、worktree、远端 SHA 与主工作区脏文件基线。
- [ ] 提交规划工件，合并 `origin/master` 并解决本地分叉冲突。
- [ ] 以 `--no-commit` 合并 `upstream/main`，按冲突策略逐文件收口。
- [ ] 检查所有上游独有提交已进入结果，且本地独有提交仍可达。
- [ ] 运行 `git diff --check`、Rust 格式/编译检查和前端 TypeScript 检查。
- [ ] 复核差异与任务验收项，完成同步合并提交和 Trellis 收尾工件。
- [ ] 在主工作区 fast-forward 到任务分支，确认用户脏文件未变化。
- [ ] 删除隔离 worktree 和临时任务分支；保留已配置的 `upstream` 远端。
