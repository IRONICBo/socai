pub mod entities;
pub mod history;
mod media_manifest;
pub mod page;
mod page_diagnostics;
pub mod tools;

use crate::sites::registry::SiteSpec;

/// Durable XHS guidance embedded into the site capability manifest.
pub const XHS_KNOWLEDGE: &str = include_str!("knowledge.md");

/// Compile-time capability manifest. Implementation files below this module
/// remain private choices of the XHS integration.
pub static XHS_SITE: SiteSpec = SiteSpec {
    id: "xhs",
    about: "Xiaohongshu (xiaohongshu.com)",
    domains: &[
        "xiaohongshu.com",
        "*.xiaohongshu.com",
        "xhslink.com",
        "*.xhslink.com",
    ],
    home_url: page::XHS_HOME_URL,
    knowledge: XHS_KNOWLEDGE,
    browser_tools: Some(&page::XHS_BROWSER_TOOLS),
    agent_tools: |page, llm| Box::pin(tools::xhs_agent_tools(page, llm)),
    default_agent_tools: Some(|page, llm| Box::pin(tools::xhs_default_agent_tools(page, llm))),
    commands: tools::xhs_commands(),
};

pub use self::entities::{parse_count_text, XhsAuthorProfile, XhsNote, XhsNoteCard};
pub use self::history::{HistoryEntry, HistorySnapshot, XhsHistoryStore};
pub use self::page::{ReadNoteOptions, XhsPageRuntime, XHS_HOME_URL};
pub use self::tools::{
    author_scan_command, close_open_note, ensure_search_ready, search_command,
    xhs_agent_instructions, xhs_agent_tools, xhs_default_agent_tools,
    xhs_macro_tools_with_llm_provider, xhs_tools, xhs_tools_with_llm_provider,
};
