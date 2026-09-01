# socai Xiaohongshu research workflow

Use socai as a read-first research tool. It drives the user's own usable Chrome
profile and returns structured JSON; it is not a bulk crawler or a publishing
tool.

## Workflow

1. Clarify the research question and the evidence needed.
2. Discover broadly with `socai_xhs_search(preview=true, num_notes=10..30)`.
   Apply `publish_time`, `note_type`, or `sort` filters only when they match the
   user's question.
3. Select a small, diverse set of useful cards. Do not treat likes alone as
   evidence of quality or representativeness.
4. Deep-read selected cards with `socai_xhs_get_notes`. Request only the
   comments, OCR, transcription, or media that the analysis actually needs.
5. For competitor or creator research, use `socai_xhs_author(preview=true)`
   first, then deep-read a small sample.
6. Synthesize findings with links/ids and distinguish observed evidence from
   your inference. Mention sampling and platform-bias limits.

## Operational rules

- Do not call socai tools in parallel: they share a browser profile and daemon.
- Start small. Increase `num_notes` only when the first sample is insufficient.
- Use preview for discovery; use deep reads for claims about post bodies,
  comments, images, or video.
- If the CLI is missing, show the returned SocAI install/product URL. Do not
  install software without the user's request.
- If login or page verification blocks the run, ask the user to make the
  configured Chrome profile usable and retry; do not bypass platform controls.
- Keep usage personal-scale, read-first, and consistent with platform rules.
