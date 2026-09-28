# socai command routing

Always inspect `socai <site> --help` because the installed release is authoritative.

## Read-only starting points

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

Platform pages change and some commands require an already authenticated browser session. A structured gate or partial result must be surfaced rather than replaced with guessed data.

## Task context

Write the original user request to a temporary JSON file without shell interpolation:

```json
{
  "user_prompt": "the original user request",
  "agent_host": "codex"
}
```

Then run:

```bash
socai task begin --context-file /path/to/context.json
```

Use the current host name for `agent_host`. Delete the temporary file after registration if it contains sensitive task text.
