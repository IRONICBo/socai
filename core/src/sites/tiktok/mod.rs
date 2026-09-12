pub mod entities;
pub mod page;
pub mod tools;

use crate::sites::registry::SiteSpec;

/// Stable, file-backed sink for verified TikTok learnings.
pub const TIKTOK_KNOWLEDGE: &str = include_str!("knowledge.md");

/// Compile-time capability manifest. Implementation files below this module
/// remain private choices of the TikTok integration.
pub static TIKTOK_SITE: SiteSpec = SiteSpec {
    id: "tiktok",
    about: "TikTok (tiktok.com)",
    domains: &["tiktok.com", "*.tiktok.com"],
    home_url: "",
    knowledge: TIKTOK_KNOWLEDGE,
    browser_tools: Some(&page::TIKTOK_BROWSER_TOOLS),
    agent_tools: |page, llm| Box::pin(tools::tiktok_agent_tools(page, llm)),
    default_agent_tools: None,
    commands: tools::TIKTOK_COMMANDS,
};

pub use self::entities::{
    tiktok_author_url, tiktok_video_url, TikTokAuthorProfile, TikTokComment, TikTokVideo,
    TikTokVideoCard,
};
pub use self::page::{TikTokPageRuntime, TIKTOK_HOME_URL};
pub use self::tools::{
    tiktok_agent_instructions, tiktok_agent_tools, tiktok_tools, tiktok_tools_with_llm_provider,
};
