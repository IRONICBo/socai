# Socai skills-only plugin directory submission

## Scope

Submit `socai-social-research` as a skills-only plugin. It deliberately has no MCP server. The skill uses an already installed local Socai CLI and the user's authenticated Chrome session. The public listing and reviewer cases cover all six supported platforms: Xiaohongshu (XHS/RedNote/Redbook), Douyin, TikTok, Instagram, LinkedIn, and X (Twitter).

Supported execution surfaces:

- Codex CLI, Claude Code, Cursor, Gemini CLI, Kimi Code, Qwen Code, TraeCode, OpenCode, GitHub Copilot, WorkBuddy/CodeBuddy, and other local shell-enabled Agent Skills hosts;
- Coze/扣子 environments only when the selected runtime can reach the locally installed Socai CLI and authenticated Chrome session.

Unsupported execution surfaces must return an explicit prerequisite message. The plugin must not claim that it collected social-platform data when the host cannot access the local Socai CLI or Chrome session.

## Distribution readiness

| Host | Repository artifact | Local validation | Public search requirement |
| --- | --- | --- | --- |
| Codex | `plugin.json`, `.codex-plugin/plugin.json`, `.agents/plugins/marketplace.json` | Plugin schema, local reinstall, Skill validation | OpenAI organization submission and review |
| Claude Code | `.claude-plugin/marketplace.json` and per-plugin manifest | `claude plugin validate`, install, component inventory | Submit the public repository through Anthropic's plugin directory form |
| Cursor | `.cursor-plugin/marketplace.json` and per-plugin manifest | JSON/path validation and local Skill install | Submit the public repository through Cursor Marketplace review |
| WorkBuddy | `plugins/workbuddy/dist/socai-social-research.zip` | WorkBuddy frontmatter validation and archive inspection | Upload, review, and publish in the Skill Marketplace |
| CodeBuddy | `.codebuddy-plugin/marketplace.json` and per-plugin manifest | JSON/path validation, host archive inspection, and direct Skill install | Add the Git repository marketplace or submit it to the applicable CodeBuddy catalog |
| Coze/扣子 | `plugins/socai-social-research/dist/socai-social-research-skills-only.zip` | Import-format validation only; local execution bridge not implemented | Blocked until an approved local application/device bridge is implemented and reviewed |
| Doubao | Same Coze source package | Not applicable for new channel publication | No new low-code Agent channel submission is currently available |
| Gemini CLI | Standard Skill plus `gemini-extension.json` host archive | Isolated workspace installation/status check and archive inspection | Publish a dedicated public extension repository/release and add the `gemini-cli-extension` topic for Gallery indexing |
| Kimi Code | `kimi.plugin.json` host archive plus direct Skill install | Manifest/archive inspection and isolated workspace installation | Install by URL/ZIP; curated directory inclusion requires Kimi partner review |
| Qwen Code | Agent Plugin v1 ZIP plus direct Skill install | Agent Plugin schema and isolated workspace installation | Install from Git/archive; it can also consume Gemini and Claude extension catalogs |
| TraeCode | Portable Skill upload ZIP plus direct Skill install | Archive inspection and isolated workspace installation | Import the ZIP or enable the project `.agents/skills/` directory; public community listing remains platform-controlled |
| OpenCode | Standard Skill installed by `socai integrate` | Isolated workspace installation/status check | No global marketplace is implied; distribute through CLI or repository |
| GitHub Copilot | Standard Skill installed by `socai integrate` | Isolated workspace installation/status check | Repository or personal Skill installation; no Socai marketplace claim |
| Generic Agent Skills | Standard Skill under `.agents/skills/` | Isolated workspace installation/status check | Host-specific directory or catalog publication |

## Build

```bash
python3 plugins/socai-social-research/scripts/build_skills_only_zip.py
python3 plugins/socai-social-research/scripts/build_host_packages.py
```

The generated archives are written to `plugins/socai-social-research/dist/`. The skills-only Agent Plugin excludes marketplace files, local cache metadata, MCP configuration, and host compatibility manifests. Separate host archives put the required CodeBuddy, Gemini, Kimi, or Trae entry file at the archive root while reusing the same portable Skill content.

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

The same repository contains `.claude-plugin/plugin.json` and the portable skill. It can be distributed immediately through a public Git repository marketplace for Claude Code, or uploaded to an organization marketplace. Inclusion in Anthropic's global directory is a separate review and publication process; a Git marketplace alone does not guarantee global proactive discovery. After the public branch is available, the repository owner submits it through `https://platform.claude.com/plugins/submit`.

Validate and install the repository marketplace locally before submission:

```bash
claude plugin validate .
claude plugin marketplace add https://github.com/socai-io/socai.git
claude plugin install socai-social-research@socai
claude plugin list
```

## Cursor distribution

The root `.cursor-plugin/marketplace.json` lists `plugins/socai-social-research`, and that plugin contains both the open-standard root `plugin.json` and a Cursor manifest. This lets a checked-out repository act as a marketplace and gives Cursor's public review the platform-specific discovery metadata.

Before public submission:

1. Install the repository as a local plugin and test the discovery prompts below in a new Cursor conversation.
2. Confirm the six platform names and research-intent keywords find the plugin in Customize.
3. Push the branch to a public repository revision.
4. Submit the repository at `https://cursor.com/marketplace/publish`.

Cursor reviews marketplace submissions. The manifest and branch alone do not make the plugin publicly searchable.

## WorkBuddy distribution

Run:

```bash
cd plugins/workbuddy
./build.sh social-skill
```

Upload `dist/socai-social-research.zip` from 【专家·技能·连接器】→【技能】→【添加技能】→【创建技能】. Keep all six platform names plus `社媒聆听`, `消费者洞察`, and `竞品研究` in the listing. Public search requires WorkBuddy review and publication; the existing Xiaohongshu-only skill and expert remain separate packages.

## CodeBuddy, Kimi, Gemini, Qwen, and Trae distribution

- CodeBuddy can add this repository as a marketplace because the root now contains `.codebuddy-plugin/marketplace.json`; its plugin uses `plugins/socai-social-research/.codebuddy-plugin/plugin.json`.
- Kimi Code can install `socai-social-research-kimi.zip`, whose root contains `kimi.plugin.json` and `skills/`.
- Gemini CLI can install `socai-social-research-gemini.zip` as an extension. Global Gallery indexing still requires a dedicated public repository or release whose archive root contains `gemini-extension.json`, plus the `gemini-cli-extension` GitHub topic.
- Qwen Code can install `socai-social-research-skills-only.zip` directly as Agent Plugins v1; it can also consume compatible Gemini or Claude distribution sources.
- TraeCode can import `socai-social-research-trae.zip`, or discover the project copy installed under `.trae/skills/` or `.agents/skills/`.

Build all host archives with:

```bash
python3 plugins/socai-social-research/scripts/build_host_packages.py
```

These entry points make the Skill installable and locally discoverable. Public search still depends on the relevant platform crawler, marketplace, account, and review status.

## Coze and Doubao distribution

Coze/扣子 can import a ZIP that follows Agent Plugins 1.0.0, so the same archive can be used to validate metadata and Skill parsing:

```bash
python3 plugins/socai-social-research/scripts/build_skills_only_zip.py
```

This archive is not yet an operational Coze integration. It contains no Coze local-application/device bridge, so a hosted Coze runtime cannot reach the user's local Socai CLI or authenticated Chrome session. Do not publish it as a working data connector. First implement and obtain review for the local execution bridge, declare that dependency in Coze, then test both connected execution and fail-closed cloud-only behavior before submitting it to a team or enterprise store.

As of 2026-09-29, Coze's official update notes state that newly created low-code agents can no longer be published to the Doubao channel. Therefore this repository can prepare and validate the Coze plugin, but it must not claim a new public Doubao-client listing. A future direct Doubao channel requires ByteDance to restore or replace that publication route.

## Direct Agent Skills hosts

The CLI installer writes the same portable skill into each host's documented discovery directory:

```bash
socai integrate install cursor
socai integrate install gemini-cli
socai integrate install kimi-code
socai integrate install qwen-code
socai integrate install trae-code
socai integrate install codebuddy
socai integrate install opencode
socai integrate install github-copilot
socai integrate install all
socai integrate status --json
```

These installs make Socai discoverable to that user's local agent. They do not publish a global marketplace listing.

## Discovery validation

Validate with a clean host where Socai is not named in the prompt:

1. Search each available platform plugin or skill directory for `Xiaohongshu`, `XHS`, `RedNote`, `Redbook`, `Douyin`, `TikTok`, `Instagram`, `LinkedIn`, `X`, `Twitter`, `social listening`, `consumer insights`, `competitor research`, and `social media research`.
2. Confirm that Socai appears for relevant data-research queries and does not rank for social-content creation.
3. Install the plugin, start a new conversation, and run all five positive and three negative cases.
4. Repeat once with Socai installed and connected, and once without the local CLI, to validate both success and fail-closed behavior.
