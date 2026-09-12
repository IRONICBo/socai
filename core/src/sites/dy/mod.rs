pub mod entities;
pub mod page;
pub mod tools;

use crate::sites::registry::SiteSpec;

/// Stable, file-backed sink for verified Douyin learnings.
pub const DY_KNOWLEDGE: &str = include_str!("knowledge.md");

/// Compile-time capability manifest. Implementation files below this module
/// remain private choices of the Douyin integration.
pub static DY_SITE: SiteSpec = SiteSpec {
    id: "dy",
    about: "Douyin (douyin.com)",
    domains: &[
        "douyin.com",
        "*.douyin.com",
        "iesdouyin.com",
        "*.iesdouyin.com",
    ],
    // Let Douyin tools own first navigation so they can use a much longer
    // timeout for the site's occasional 4-5 minute blank-page throttling.
    home_url: "",
    knowledge: DY_KNOWLEDGE,
    browser_tools: Some(&page::DOUYIN_BROWSER_TOOLS),
    agent_tools: |page, llm| Box::pin(tools::dy_agent_tools(page, llm)),
    default_agent_tools: None,
    commands: tools::DY_COMMANDS,
};

pub use self::entities::{
    douyin_author_url, douyin_video_url, DouyinAuthorProfile, DouyinComment, DouyinVideo,
    DouyinVideoCard,
};
pub use self::page::{DouyinPageRuntime, DOUYIN_HOME_URL};
pub use self::tools::{
    dy_agent_instructions, dy_agent_tools, dy_tools, dy_tools_with_llm_provider,
};
