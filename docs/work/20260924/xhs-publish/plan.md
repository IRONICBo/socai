# 小红书真实发帖与持久 CDP 接入计划

## 目标

在不调用小红书私有接口、不读取 Cookie、不启动新 Chrome、不反复建立浏览器调试连接的前提下，复用 Deeptensor 现有 Chrome，完成图片笔记的上传、填写、一次性发布、发布后核对和测试笔记清理。

## 命令契约

```bash
socai xhs prepare-publish \
  --media /absolute/path/image.jpg \
  --title "测试标题" \
  --text "测试正文"

socai xhs commit-action --action-id <ACTION_ID>
socai xhs reconcile-action --action-id <ACTION_ID>
```

## 执行设计

1. 复用 socai daemon 持有的浏览器 WebSocket 和 XHS 标签页；daemon 空闲保活 24 小时，一次 daemon 生命周期只建立一条浏览器连接。
2. `prepare-publish` 校验 1–18 张 JPG/JPEG/PNG/WebP、标题 1–20 字、正文 1–1000 字，并计算每个素材的 SHA-256。
3. 使用 `DOM.setFileInputFiles` 操作真实文件输入框，再触发页面框架的标准 `input/change` 监听，不调用上传 API。
4. 使用 CDP 聚焦和键盘输入填写标题、正文，随后逐字回读标题、正文和媒体数量。
5. 创建持久 action receipt：`draft -> prepared -> committing -> committed/commit_unknown -> reconciled`。
6. 账号校验只组合浏览器 context、页面可见昵称和头像路径生成不可逆标识；不读取 Cookie、`localStorage` 或 `sessionStorage`。
7. `commit-action` 在最终点击前重新校验账号、内容、媒体签名和按钮状态，并在磁盘锁内先写入 `committing`。
8. 使用稳定的 CDP `backendNodeId` 核对同一最终发布控件，并以中心点命中测试阻止遮挡或替换控件。
9. 最终红色“发布”按钮只允许一次 CDP 指针点击；断连、超时和进程退出均不得自动重试。
10. `reconcile-action` 只读扫描完整笔记管理页，以新 note ID、精确标题和发布时间核对发布结果并保存截图。
11. daemon 返回普通业务错误时直接透传，不再误判为连接失败并启动新 daemon；只有传输失败或构建版本变化才重连。
12. Chrome 首次外部调试授权如出现，由执行端通过本机 UI 一次性确认；授权后 prepare/commit/reconcile 均复用同一条 WebSocket，不再向用户重复请求。

## 验收标准

- Chrome 不因每个步骤重新弹出调试授权；prepare/commit/reconcile 复用同一 daemon、PID 和 WebSocket。
- prepare 阶段绝不点击最终发布。
- 同一 action id 的 commit 最多产生一次最终点击。
- 成功页、笔记管理页、note ID 和截图证据与 receipt 一致。
- 真实测试笔记通过精确标题和 note ID 删除，删除后管理页计数恢复且标题消失。
- CLI 帮助、Rust 类型检查、相关测试、格式和 diff 检查通过。
- 自动代码审查没有未解决的高风险问题。

## 边界

- 本次实现图片笔记；视频发布不伪装为已支持。
- 小红书创作页使用网页交互，不使用逆向私有 API。
- 管理页未提供可稳定取得的新公开笔记 URL 时，不拿旧标签页 URL 冒充发布结果。
- 笔记处于平台审核期时，以创作平台成功页和笔记管理页作为回读依据，不伪造公开页结果。
