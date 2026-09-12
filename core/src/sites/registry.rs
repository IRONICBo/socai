//! Site registry — the single wiring point for site capabilities.
//!
//! Each site module exposes one `pub static <ID>_SITE: SiteSpec` and gets
//! listed in [`all_sites`]. `SiteSpec` is the compile-time capability manifest:
//! it declares the site's domains, durable knowledge, browser-context tools,
//! host tools, and CLI commands without prescribing a fixed source-file layout.
//! Interactive hosts select a registered spec explicitly; the registry never
//! infers or switches platforms from task text.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;

use crate::agent::tool::ToolProgressSender;
use crate::agent::{Backend as LlmProvider, Tool};
use crate::cdp::PageSession;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = anyhow::Result<T>> + Send>>;

/// Async factory: build the site's agent tools against a shared page.
pub type AgentToolsFn = fn(Arc<PageSession>, Arc<dyn LlmProvider>) -> BoxFuture<Vec<Arc<dyn Tool>>>;
/// Compatibility alias for callers that compose site instructions directly.
pub type AgentInstructionsFn = fn(&str) -> String;

/// Browser-context capability bundle embedded into the binary by a site.
///
/// This is the Rust equivalent of an ego-lite `browserTools` manifest entry:
/// the platform owns the JavaScript and its allow-list, while the shared
/// runtime only performs validated loading and invocation.
pub struct BrowserToolset {
    pub binding: &'static str,
    pub source: &'static str,
    pub tools: &'static [&'static str],
}

impl BrowserToolset {
    async fn run(
        &self,
        page: &PageSession,
        site_id: &str,
        name: &str,
        arg: Option<&Value>,
    ) -> anyhow::Result<Value> {
        if !self.tools.contains(&name) {
            anyhow::bail!("Unknown {site_id} browser tool: {name}");
        }

        let tool_name = serde_json::to_string(name)?;
        let invocation = match arg {
            Some(value) => format!("__socaiBrowserTool({})", serde_json::to_string(value)?),
            None => "__socaiBrowserTool()".to_string(),
        };
        let trace_label = serde_json::to_string(&format!("{site_id}/{name}"))?;
        let expression = format!(
            "(function() {{\n{}\n// SOCAI_BROWSER_TOOL\n\
             const __socaiBrowserToolTrace = {trace_label};\n\
             const __socaiBrowserTools = {};\n\
             const __socaiBrowserTool = __socaiBrowserTools[{tool_name}];\n\
             if (typeof __socaiBrowserTool !== 'function') {{\n\
               throw new Error('Browser tool is not callable: ' + {tool_name});\n\
             }}\n\
             return {invocation};\n\
             }})()",
            self.source, self.binding
        );
        page.evaluate_json(&expression).await
    }
}

/// One-shot CLI/daemon command: `(page, JSON args, debug_snapshot, progress)` → JSON.
pub type CommandRunFn =
    fn(Arc<PageSession>, Value, bool, Option<ToolProgressSender>) -> BoxFuture<Value>;

pub struct SiteSpec {
    /// Short site id — doubles as the CLI subcommand (`socai <id> <tool>`),
    /// the daemon `site` field, and the `enabled_sites` gate value.
    pub id: &'static str,
    pub about: &'static str,
    /// Host patterns declared for discovery and diagnostics. Security-sensitive
    /// URL validation remains platform-owned because URL forms differ by site.
    pub domains: &'static [&'static str],
    pub home_url: &'static str,
    /// Durable, platform-owned instructions embedded at compile time. The file
    /// may start empty, but remains a stable sink for verified site learnings.
    pub knowledge: &'static str,
    /// Page-context DOM capabilities. Host orchestration remains in Rust tools.
    pub browser_tools: Option<&'static BrowserToolset>,
    pub agent_tools: AgentToolsFn,
    /// Optional default tool surface for normal app/TUI agents. Sites can keep
    /// a broader command/debug surface in `agent_tools` while exposing a
    /// smaller, product-safe macro surface to interactive users.
    pub default_agent_tools: Option<AgentToolsFn>,
    pub commands: &'static [SiteCommand],
}

/// A one-shot command exposed both as a CLI subcommand and a daemon command.
pub struct SiteCommand {
    pub name: &'static str,
    /// Underlying tool name, reported in telemetry (may differ from `name`).
    pub tool_name: &'static str,
    pub about: &'static str,
    pub args: &'static [CommandArg],
    /// Whether the client should budget the long command timeout.
    pub slow: SlowWhen,
    pub run: CommandRunFn,
}

/// Declarative CLI argument. The CLI builds clap args from these and collects
/// matches into the JSON `args` object sent to the daemon, keyed by `key`.
pub struct CommandArg {
    /// JSON key in the command args object.
    pub key: &'static str,
    /// CLI flag name (`--<long>`); `None` makes this a positional argument.
    pub long: Option<&'static str>,
    pub value_name: &'static str,
    pub help: &'static str,
    pub required: bool,
    pub kind: ArgKind,
}

pub enum ArgKind {
    Str,
    /// Repeatable string flag collected into a JSON array, preserving order.
    StrList,
    Int,
    /// Boolean `--flag`; sent as `true` only when set.
    Flag,
    /// Repeatable `key=value` flag collected into a JSON object.
    KeyValueMap,
}

pub enum SlowWhen {
    Never,
    Always,
    /// Slow only when the named arg was provided (e.g. deep scrolling).
    ArgPresent(&'static str),
}

impl SlowWhen {
    pub fn applies(&self, args: &Value) -> bool {
        match self {
            SlowWhen::Never => false,
            SlowWhen::Always => true,
            SlowWhen::ArgPresent(key) => args.get(key).is_some_and(|value| !value.is_null()),
        }
    }
}

impl SiteSpec {
    /// Compose host instructions with this site's durable knowledge.
    pub fn agent_instructions(&self, extra: &str) -> String {
        let knowledge = self.knowledge.trim();
        let extra = extra.trim();
        match (extra.is_empty(), knowledge.is_empty()) {
            (true, true) => String::new(),
            (true, false) => knowledge.to_string(),
            (false, true) => extra.to_string(),
            (false, false) => format!("{extra}\n\n{knowledge}"),
        }
    }

    /// Execute one browser-context capability declared by this manifest.
    pub async fn run_browser_tool(
        &self,
        page: &PageSession,
        name: &str,
        arg: Option<&Value>,
    ) -> anyhow::Result<Value> {
        let tools = self
            .browser_tools
            .ok_or_else(|| anyhow::anyhow!("Site {} has no browser tools", self.id))?;
        tools.run(page, self.id, name, arg).await
    }

    pub fn command(&self, name: &str) -> Option<&'static SiteCommand> {
        self.commands.iter().find(|cmd| cmd.name == name)
    }
}

/// Every registered site. Site order is also CLI help order.
static SITES: &[&SiteSpec] = &[&crate::sites::xhs::XHS_SITE, &crate::sites::dy::DY_SITE];

pub fn all_sites() -> &'static [&'static SiteSpec] {
    SITES
}

pub fn find_site(id: &str) -> Option<&'static SiteSpec> {
    all_sites().iter().copied().find(|site| site.id == id)
}

/// Extract a required non-empty string arg from a command args object.
pub fn required_string(args: &Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("missing required argument: {key}"))
}
