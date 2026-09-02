# DSH integration feasibility

Checked on 2026-08-31 through 2026-09-02 (Asia/Shanghai).

## Verdict

**Feasible as a standalone DSH acquisition/integration plugin with a local CLI
backend.**

The correct boundary for SocAI today is:

```text
DSH discovery/catalog
  -> typed socai_xhs_* tool
  -> local socai CLI (JSON stdout)
  -> SocAI browser daemon + user's usable Chrome profile
  -> structured result + campaign-tagged socai.io link
```

This preserves the existing product architecture and gives DSH users a useful
first success instead of a promotional-only wrapper.

## Evidence baseline

| Source | Revision/version inspected | Relevant evidence |
| --- | --- | --- |
| `deepseek-ai/deepseek-harness` | `0a53fb55bea101816fa226bb964ae2bed71c343b` | Official bundle manifest, `dsh plugin --profile ... add`, tool and Skill APIs; developer-preview compatibility warning |
| npm `@deepseek-ai/dsh` | `0.1.1-rc.2` (`latest`) | Public minimum compatibility target; exact source tag used for build/test/profile validation |
| local `socai` | `9a4fb56cc05c5aef23138e9a3f293a91cb3fbc49` | `xhs search`, `xhs author`, and `xhs get-notes` emit JSON on stdout and share the existing Rust daemon/runtime |

The minimum supported SocAI release is `0.4.12`: repository tag inspection
confirmed that it is the first release containing the complete three-command
contract used here (`search`, `author`, and `get-notes`). The plugin checks
`socai --version` before its first browser operation.

## Integration contract

| Capability | SocAI implementation | Decision |
| --- | --- | --- |
| Installable `dsh.bundle` + `cordis.patch.yml` | Same | Directly align |
| Typed tool registration | Typed wrappers around stable CLI commands | Directly align |
| Bundled task Skill | `socai-xhs-research` workflow | Directly align |
| System prompt teaches tool routing | Prefer SocAI for Xiaohongshu research | Directly align |
| Compiled `lib/` for git install | Commit generated bundle before public release | Directly align |
| Execution backend | Local CLI + browser daemon | Preserve the existing SocAI boundary |
| Inline React renderer/card | Structured native tool result | Defer; unnecessary for acquisition MVP |

## Acquisition funnel

1. Discovery: publish a standalone public repository with `dsh-plugin` topic and
   submit it to relevant DSH community directories.
2. Activation: one DSH install command exposes the SocAI tools and Skill.
3. Value: preview search works through the user's existing SocAI/Chrome setup.
4. Conversion: successful and missing-CLI results expose a tagged `socai.io`
   link; the website leads to CLI/desktop downloads.
5. Measurement: use `utm_source=deepseek-harness`, `utm_medium=plugin`, and
   `utm_campaign=dsh-socai`; correlate with existing privacy-preserving SocAI
   install/tool telemetry at aggregate level.

Do not make the plugin a link-only advertisement. DSH registries and users are
more likely to retain a plugin that provides immediate, inspectable value.

## Discovery and marketplace status

- DeepSeek Harness's official Community plugins link currently resolves to the
  GitHub `dsh-plugin` topic. The standalone
  [`socai-io/dsh-socai`](https://github.com/socai-io/dsh-socai) repository is
  public and carries that topic, plus `deepseek-harness`, `socai`,
  `xiaohongshu`, and `browser-automation`.
- The in-Harness `dsh-market` catalog reads the community-maintained
  `awesome-dsh-plugin` registry. A listing PR adds one YAML file, but its CI
  requires a repository to be at least one day old and to have at least ten
  commits. Do not manufacture empty commits to bypass that anti-spam gate;
  submit after the standalone repository satisfies it through normal work.
  GitHub Releases provide a version-independent
  `dsh-socai.tgz` asset suitable for the registry's optional `tarball` field.
- The independent DSH Plugin Registry at `dshplugin.app` has a separate manual
  quality-gate submission form. Treat it as an additional distribution channel,
  not an official DeepSeek endorsement.

## Risks and controls

| Risk | Impact | Current control / required action |
| --- | --- | --- |
| DSH is a developer preview with breaking changes | Plugin can stop loading | Pin a tested DSH release/commit in CI; smoke-test each DSH release before updating compatibility claims |
| SocAI is not installed or DSH does not inherit its PATH | First call fails | Actionable install URL and configurable absolute `binaryPath` |
| Xiaohongshu login/profile is unusable | Browser task fails | Preserve SocAI's own profile/auth flow; never bypass controls |
| Large note bodies/comments overflow model context | Slow/costly result | Preview-first Skill, note/comment caps, 64 KB raw and final-rendered-result guard |
| Concurrent calls fight over one browser | Flaky navigation | Plugin-wide abort-aware queue across DSH sessions plus non-concurrency-safe tool declarations; separate DSH processes converge on SocAI's serialized daemon request mutex |
| Tool arguments become command injection | Local-code risk | Spawn executable with an argv array and no shell; regression test metacharacters |
| Local run paths reveal usernames to the model | Privacy leak | Return only a short SHA-256-derived `run_id`, never the absolute `run_dir` or its path segments |
| Promotional link feels spammy | Retention/reputation risk | Keep it as structured attribution after useful data; evaluate an error-only/once-per-session mode before launch |
| Public remote execution is assumed | Security/product mismatch | Document that v0.1 is local; Pro remote browser remains authenticated through SocAI itself |

## Validation completed in this spike

- Build and strict typecheck pass against DSH `0.1.2-alpha.2` source; on macOS,
  18 tests pass and the Windows-only process-tree test is registered but skipped.
- The exact public `dsh-v0.1.1-rc.2` source tag was installed and built; the
  the same strict typecheck and 19-test matrix passes against it (18 passing on
  macOS, one Windows-only skip). Both a linked
  directory and the packed `.tgz` compose into a temporary web profile and
  reach a live `dsh web` boot.
- The tests cover real DSH tool-registry execution, argv injection resistance,
  split UTF-8, bounded cancellation/timeout and Windows tree cleanup,
  cross-session serialization, stable/prerelease SocAI version gating,
  model-visible result limits, error-path redaction, and input validation.
- `publint` passes, the npm tarball contains only runtime/package assets, and
  both a local-directory install and packed-tarball install boot in temporary
  DSH profiles against `0.1.2-alpha.2`.
- A fresh `0.1.1-rc.2` profile installs directly from the public standalone Git
  URL, composes the `dsh-socai` row, boots `dsh web`, and serves HTTP 200. The
  standalone repository's CI passes against both supported DSH revisions.
- The public npm CLI executable for `0.1.1-rc.2` was not treated as evidence:
  its one-off `npx` install was stopped after prolonged high resource use. The
  exact public source tag is validated separately instead.

## Go-live checklist

- [x] Finish build, tests, pack, and profile boot against the exact public
  `0.1.1-rc.2` source tag.
- [x] Generate `lib/index.js` and `lib/index.d.ts` so git installs need no
  build-script permission; commit them when this subdirectory becomes a repo.
- [x] Integrate it as the `dsh-socai/` package directory on the
  `feat/dsh-socai-integration` branch and split the same source into the public
  `socai-io/dsh-socai` repository for root-level git installation.
- [ ] Confirm the `@socai-io` npm scope before npm publication, or keep the
  GitHub install path.
- [x] Add repository topics: `deepseek-harness`, `dsh-plugin`, `socai`,
  `xiaohongshu`.
- [x] Add CI for both pinned supported DSH source revisions.
- [x] Publish GitHub Releases with the precompiled, stable-name
  `dsh-socai.tgz` asset.
- [ ] After the repository is at least one day old and has ten meaningful
  commits, submit one `tools` entry to `awesome-dsh-plugin`; its downstream
  `dsh-market` catalog updates automatically after merge.
- [ ] Submit the standalone repository to the independent `dshplugin.app`
  quality gate if that additional channel is desired.
- [ ] Add a Windows DSH + SocAI profile smoke test; the current executable
  fixture integration suite is macOS/Linux-only.
- [ ] Decide attribution frequency after testing user experience.
- [ ] Verify the tagged website link and aggregate acquisition dashboard.
- [ ] Publish clear privacy, platform-use, and local-browser expectations.

## Production recommendation

Ship v0.1 with only the three read-first Xiaohongshu tools. Add Douyin only
after the primary funnel is measured. Defer custom web cards and remote hosted
execution until a concrete DSH user need justifies their maintenance and
security cost.
