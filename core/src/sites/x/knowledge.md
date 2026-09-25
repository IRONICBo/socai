# X research workflow

This package is read-only by default. It may reply only when the user explicitly requests the exact target post and reply content. Never create standalone posts, repost, like, bookmark, follow, unfollow, or send a message.

## Gates

- Treat `/i/flow/login` and `/i/jf/onboarding/web?...mode=login` as `login_required`.
- Stop on challenge, consent, captcha, or rate-limit states. Do not retry them in a loop.
- A search or profile that has not hydrated visible post articles is not a successful empty result.

## Reading

- Use `searchResults` for keyword timelines and `profilePosts` for a profile timeline.
- Use `postDetail` only after the active URL and root article identify the same status id.
- Replies are posts in the active conversation other than the root post; they are returned by `comments`.
- Media URLs and metrics are best-effort visible-page evidence. Missing values remain null or empty rather than being inferred.

## Navigation

- Prefer the current logged-in browser session.
- Search, profile, and explicit post URLs are valid entry routes for CLI workflows.
- Do not turn a login redirect, challenge, wrong post, or unhydrated page into an empty success.

## Explicit replies

- Use `reply` only for an explicit user-authorized write. Preserve the requested text and target; do not invent additional replies.
- The command uses visible CDP pointer and keyboard events. It never calls a platform write API, never replaces a non-empty draft, and dispatches the Reply click at most once.
- Treat `commit_unknown` as unknown, not as permission to retry. Verify the active post id, signed-in author, exact rendered text, and conversation relationship before reporting success.
