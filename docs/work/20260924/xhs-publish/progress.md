# 小红书真实发帖接入进度

更新时间：2026-09-25 03:00（Asia/Shanghai）

## 已完成

- 分支：`feat/xhs-publish-20260924`
- 工作树：`docs/work/20260924/worktrees/xhs-publish`
- 增加 CLI：`prepare-publish`、`commit-action`、`reconcile-action`。
- 增加进程安全、原子落盘的 action receipt 和全局幂等键。
- 增加 CDP 文件输入、字段聚焦、扁平 DOM 最终按钮定位、中心点命中检查和稳定 `backendNodeId` 校验。
- prepare/commit/reconcile 复用 Deeptensor 现有 Chrome、daemon 的同一浏览器 WebSocket 和站点标签页，不启动新 Chrome。
- daemon 空闲保活从 3 小时提升到 24 小时。
- 修复普通业务错误被误判为 daemon 不可用的问题；错误不再触发重复 daemon、重复 WebSocket 或重复授权。
- daemon 目录和 Unix socket 权限分别固定为 `0700`、`0600`。
- 账号标识只使用 browser context、页面可见昵称和头像路径；未读取 Cookie、`localStorage` 或 `sessionStorage`。
- 首次 Chrome 外部调试确认由执行端通过 macOS 辅助功能点击 `Allow`，未要求用户手动操作；后续三步未再次授权或重连。
- Rust 类型检查：`cargo check -p socai-core -p socai-cli` 通过。
- 两轮自动代码审查已完成，发现的幂等、身份漂移、媒体校验、按钮竞态和 IPC 权限问题均已修复。

## 真实页面验证

生产 CLI 使用 action `b460907550399023d399770621ecffc29ac0647d06262b76e79ae443189dca66`，在同一 daemon PID 和 WebSocket 上完成：

1. 上传 `site/public/social-card.png`，编辑器显示 `1/18`。
2. 填写并回读标题 `socai CLI实发验证0925` 和正文。
3. 最终“发布”按钮在 CDP 扁平 DOM 中确认为 `button.ce-btn.bg-red` 且可用。
4. 在提交前锁定状态，只发送一次最终点击。
5. 成功页显示“发布成功”。
6. 笔记管理页回读到精确标题、时间 `2026-09-25 02:56` 和新 note ID `6ab5727b000000001303e8a5`。
7. 通过真实页面删除确认框只点击一次，删除后管理页从 11 条恢复为 10 条，精确标题消失。

生产 receipt 与截图：

- `~/.socai/actions/receipts/b460907550399023d399770621ecffc29ac0647d06262b76e79ae443189dca66.json`
- `~/.socai/actions/evidence/b460907550399023d399770621ecffc29ac0647d06262b76e79ae443189dca66/prepared-before-submit.png`
- `~/.socai/actions/evidence/b460907550399023d399770621ecffc29ac0647d06262b76e79ae443189dca66/before-final-click.png`
- `~/.socai/actions/evidence/b460907550399023d399770621ecffc29ac0647d06262b76e79ae443189dca66/after-final-click.png`
- `~/.socai/actions/evidence/b460907550399023d399770621ecffc29ac0647d06262b76e79ae443189dca66/note-manager-reconcile.png`

删除证据目录：`/Users/asklv/Projects/socai/docs/work/20260923/live-write-validation/`

- `xhs-cli-publish-delete-live.json`
- `screenshots/31-xhs-cli-before-delete.png`
- `screenshots/32-xhs-cli-delete-confirmation.png`
- `screenshots/33-xhs-cli-after-delete.png`
- `screenshots/34-xhs-cli-after-delete-settled.png`

同一目录还保留了独立持久 CDP 验证 `xhs-publish-delete-live.json` 及截图 `20`–`30`，其测试笔记也已删除。

## 如实保留的限制

- 测试笔记发布后处于平台审核期，没有立即得到公开详情 URL；成功页和创作平台管理页已通过新 note ID、精确标题和时间完成回读。
- 当前生产命令只接受图片，不接受视频。

## 检查结果

- `cargo check -p socai-core -p socai-cli`：通过。
- `cargo test -p socai-core --lib sites::xhs`：35 项通过。
- `cargo test -p socai-cli --bin socai`：6 项通过。
- 目标 Rust 文件格式检查：通过；仓库原有其他文件存在与本变更无关的 `cargo fmt --all --check` 差异。
- `git diff --check`：通过。
