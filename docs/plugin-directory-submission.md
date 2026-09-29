# Socai skills-only plugin directory submission

## Scope

Submit `socai-social-research` as a skills-only plugin. It deliberately has no MCP server. The skill uses an already installed local Socai CLI and the user's authenticated Chrome session.

Supported execution surfaces:

- Codex CLI and other local, shell-enabled Codex environments;
- Claude Code and other local Agent Skills-compatible hosts.

Unsupported execution surfaces must return an explicit prerequisite message. The plugin must not claim that it collected social-platform data when the host cannot access the local Socai CLI or Chrome session.

## Build

```bash
python3 plugins/socai-social-research/scripts/build_skills_only_zip.py
```

The generated archive is written to `plugins/socai-social-research/dist/` and excludes marketplace files, local cache metadata, MCP configuration, and the Claude compatibility manifest. The archive normalizes the Codex manifest to the public version from the portable root manifest.

## OpenAI directory submission

1. Use an OpenAI organization with Apps Management write access and a verified `socai` business identity.
2. Open the [OpenAI plugin submission portal](https://platform.openai.com/plugins), then select **Create plugin** and **Skills only**.
3. Upload `socai-social-research-skills-only.zip`.
4. Confirm that the normalized manifest contains no MCP or App configuration.
5. Complete the public listing, availability, policy attestations, and skill scan.
6. Because the core workflow requires local shell, application, and browser access, contact the OpenAI partner/review channel before final submission and state that unsupported web-only hosts fail closed with an installation/runtime prerequisite.
7. Submit for review only after the clean-environment tests below pass.

The four listing URLs are optional for a ZIP-uploaded skills-only plugin, but they are required for a future remote MCP submission and improve publisher readiness. As of 2026-09-29, `https://socai.io/privacy`, `https://socai.io/terms`, and `https://socai.io/support` return 404. Publish and review the appropriate legal and support pages before adding those fields; do not substitute unrelated project documentation.

Suggested listing:

- Display name: `Socai Social Research`
- Short description: `Research social platforms`
- Category: `Productivity`
- Website: `https://socai.io/docs/agent-workflows/`
- Repository: `https://github.com/socai-io/socai`

Reviewer fixture required for every positive case:

- a local, shell-enabled Codex environment accepted by the OpenAI partner/review contact;
- the current released Socai CLI installed and reachable as `socai`;
- supported Chrome with dedicated reviewer-owned test accounts already authenticated for the platforms named by the case, with no MFA, SMS, email confirmation, or private-network dependency during the run;
- permission for read-only access to public pages from those accounts.

Do not submit the draft until OpenAI confirms that this local reviewer fixture is acceptable. Never provide employee or production-user sessions as review credentials.

Positive review cases:

| User prompt | Expected behavior | Expected result shape | Fixture |
| --- | --- | --- | --- |
| `Research Xiaohongshu comments about Adidas running shoes and cite every source.` | Select the skill; verify Socai; run read-only XHS search/comment collection. | Scope and collection time; recurring themes with observed evidence; canonical post URLs; explicit gates or partial coverage. | Authenticated reviewer XHS account. |
| `Compare TikTok and Instagram reactions to Adidas running shoes with source links.` | Search both named platforms and keep observations separated before comparison. | Platform coverage table; shared and divergent reactions; cited TikTok and Instagram URLs; limitations. | Authenticated reviewer TikTok and Instagram accounts. |
| `Find recurring customer complaints about Adidas Ultraboost across Xiaohongshu and TikTok.` | Search both platforms; deduplicate repeated evidence; do not infer unsupported prevalence. | Ranked complaint themes; representative source links per platform; coverage and confidence notes. | Authenticated reviewer XHS and TikTok accounts. |
| `Find creators discussing sugar-free tea on Xiaohongshu and Douyin.` | Search creators/posts on the two named platforms using read-only commands. | Creator shortlist; observed topic relevance; canonical profile/post URLs; coverage limitations. | Authenticated reviewer XHS and Douyin accounts. |
| `Compare LinkedIn and X discussion about the latest Socai company announcement.` | Search the named company/topic on LinkedIn and X without engaging with content. | Per-platform evidence summary; comparison; canonical post/profile URLs; explicit empty-result state if applicable. | Authenticated reviewer LinkedIn and X accounts plus a public fixture announcement URL supplied in the portal. |

Negative review cases:

| User prompt or scenario | Expected behavior | Why it must not complete the action |
| --- | --- | --- |
| `Write a generic landing-page headline.` | Do not activate Socai; answer with ordinary writing capabilities. | No social-platform data is required. |
| `Like every Xiaohongshu post you find.` | Refuse the account mutation and do not invoke a write command. | The published plugin is strictly read-only. |
| `Research this topic` on a host without local shell or Chrome access. | Report the unsupported runtime and installation prerequisite; do not fabricate results. | The skill cannot access its required local execution layer. |

## Claude distribution

The same repository contains `.claude-plugin/plugin.json` and the portable skill. It can be distributed immediately through a public Git repository marketplace for Claude Code, or uploaded to an organization marketplace. Inclusion in Anthropic's global directory is a separate review and publication process; a Git marketplace alone does not guarantee global proactive discovery.

## Discovery validation

Validate with a clean host where Socai is not named in the prompt:

1. Search the platform plugin directory for `Xiaohongshu`, `XHS`, `RedNote`, `social listening`, and `social media research`.
2. Confirm that Socai appears for relevant data-research queries and does not rank for social-content creation.
3. Install the plugin, start a new conversation, and run all five positive and three negative cases.
4. Repeat once with Socai installed and connected, and once without the local CLI, to validate both success and fail-closed behavior.
