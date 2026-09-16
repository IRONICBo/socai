# LinkedIn research workflow

Use LinkedIn for public people/content research and for drafting a greeting locally. Do not send invitations, messages, reactions, replies, or comments.

## Search routes

- People: `https://www.linkedin.com/search/results/people/?keywords=<encoded query>`
- Content: `https://www.linkedin.com/search/results/content/?keywords=<encoded query>`
- All: `https://www.linkedin.com/search/results/all/?keywords=<encoded query>`

Navigate to a route with `navigate_site`, then call `searchState` before trusting `searchResults`. A redirect to `/authwall`, `/uas/login`, `/login`, or a challenge page is not an empty search result. Report the gate and ask the user to finish login or verification in the browser.

## Reading and identity

- Call `pageState` after each navigation.
- Use `profileDetail` only on `/in/<profile-id>` pages.
- Treat `current_role` as authoritative only when the page explicitly marks an experience as current. When it is `null`, `latest_role` is historical/unknown-currentness context and must not be described as the person's current job.
- Use `postDetail` and `comments` on `/posts/...` or `/feed/update/urn:li:...` pages.
- Keep the returned canonical URL and stable profile/activity id with every note or citation. Never identify a result only by its visible position.
- Visible guest post pages can contain useful post text and comments even when a sign-in overlay is present. Trust `postDetail.ok`; use `pageState.login_gate_present` to disclose that additional content may be hidden.

## Greeting drafts

Build a short draft only from facts returned by `profileDetail`, `postDetail`, or the selected search result. Mention one concrete shared topic, avoid invented familiarity, and label the output as a draft. The user must review and send it themselves because this skill exposes no connect or message action.

## Pagination and stopping

Use `scrollResults` only when the current search is valid and more evidence is needed. Re-run `searchResults`, deduplicate by `id` or canonical `url`, and stop once the requested coverage is met. Do not scroll indefinitely or treat rate limiting, login gates, or incomplete hydration as zero results.
