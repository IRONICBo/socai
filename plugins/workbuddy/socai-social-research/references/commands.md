# Socai 只读命令速查

每次执行前都用 `socai <site> --help` 核对当前安装版本的参数。

```bash
socai x search "<query>" --num 10 --pretty
socai x profile "<handle>" --num 10 --pretty
socai x get-posts --post "<x-post-url>" --num-comments 8 --pretty

socai instagram search "<query>" --num 10 --pretty
socai instagram profile "<username>" --num 10 --pretty

socai linkedin search "<query>" --type content --num 10 --pretty
socai linkedin profile "<linkedin-profile-url>" --pretty

socai tiktok search "<query>" --num 10 --pretty
socai tiktok author "<handle>" --num 10 --pretty

socai dy search "<query>" --num 10 --pretty
socai dy author "<author-id>" --num 10 --pretty

socai xhs search "<query>" --num-notes 10 --num-comments 8 --pretty
socai xhs author "<author-id>" --num-notes 10 --pretty
```

站点页面会变化，部分命令依赖已经登录的浏览器会话。结构化阻断或部分结果必须如实报告，不得用猜测数据替代。
