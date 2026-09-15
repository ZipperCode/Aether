# GitHub Pages 停用契约

## 1. Scope / Trigger

本仓库停用 GitHub Pages。2026-08-06 的 `1dd4df56b`、`89523fb88` 已移除工作流和前端 Pages 构建配置；上游合并曾重新带回工作流，用户于 2026-09-15 明确确认恢复停用。同步上游或发布版本时应保留这一决定。

## 2. Signatures

- 撤下网站：`DELETE /repos/ZipperCode/Aether/pages`，成功为 HTTP 204；仅在用户授权撤站时执行。
- 核对停用：`GET /repos/ZipperCode/Aether/pages`，已知仓库可访问且认证有效时应返回 HTTP 404。
- 公网站点：`https://zippercode.github.io/Aether/`。

## 3. Contracts

- 不保留 `.github/workflows/deploy-pages.yml` 或其他 Pages 配置、上传、部署 action。
- Rust CI 的 push / pull_request 路径过滤保留该文件名，使删除或回流仍触发检查；这不是 Pages 部署入口。
- 前端 `base: '/'`，不恢复 `GITHUB_PAGES` 分支。
- 普通 CI、Release、Nightly、版本 tag 和发布资产独立于 Pages；停用 Pages 不删除或重建这些发布内容，也不改变环境保护规则。

## 4. Validation & Error Matrix

| 结果 | 判定 |
| --- | --- |
| DELETE 204，后续 GET 404 | Pages 配置已移除 |
| DELETE 401/403 或其他失败 | 撤站未通过，保留错误证据 |
| 公网站点仍返回 200 | 尚未确认 CDN 撤站，继续核对，不能宣称网站已下线 |
| 工作流再次引入 Pages action | 与本地停用契约冲突 |

## 5. Good / Base / Bad Cases

- Good：正常发布应用 tag，同时保持 Pages 停用。
- Base：删除回流的 Pages 工作流，前端和其他发布流程保持原样。
- Bad：为消除 Pages 红灯重新开放 tag 部署、恢复网站，或移动已发布 tag。

## 6. Required Checks

- 工作流目录中没有使用 `actions/configure-pages`、`actions/upload-pages-artifact`、`actions/deploy-pages` 的步骤或 `GITHUB_PAGES` 构建开关；`git diff --check` 通过。
- 核对 Pages API 和公网 HTTP 状态；核对应用 tag 对象、提交及资产保持一致。
- 此配置删除无需新增运行时测试；通过相应提交的 GitHub CI 核对流程完整性。

## 7. Wrong vs Correct

- Wrong：上游包含 Pages 工作流就直接保留，遇到环境拒绝后放宽权限。
- Correct：先核对本地已确认的停用决定，移除回流工作流；只有用户明确要求重新启用时才设计新的发布路径。
