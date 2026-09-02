use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent::tool::ToolProgressSender;
use crate::agent::{Backend as LlmProvider, Tool, ToolContext, ToolResult};
use crate::cdp::PageSession;
use crate::sites::content::{
    author_scan_input_schema, empty_filters_schema, get_notes_input_schema,
    page_state_input_schema, search_input_schema, video_note_locator_schema, ContentCapabilities,
    ContentOperation, ContentPlatform,
};
use crate::sites::dy::DouyinPageRuntime;
use crate::sites::registry::{
    required_string, ArgKind, BoxFuture, CommandArg, SiteCommand, SiteSpec, SlowWhen,
};
use crate::sites::runner::{get_f64, get_i64, json_result, run_tool_command, ToolCommand};

pub const DY_KNOWLEDGE: &str = include_str!("knowledge.md");

pub fn dy_tools(page: Arc<PageSession>) -> Vec<Arc<dyn Tool>> {
    dy_tools_with_llm_provider(page, None)
}

pub fn dy_tools_with_llm_provider(
    page: Arc<PageSession>,
    _llm_provider: Option<Arc<dyn LlmProvider>>,
) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(SearchTool { page: page.clone() }) as Arc<dyn Tool>,
        Arc::new(PageStateTool { page }),
    ]
}

pub async fn dy_agent_tools(
    page: Arc<PageSession>,
    llm_provider: Arc<dyn LlmProvider>,
) -> anyhow::Result<Vec<Arc<dyn Tool>>> {
    let _ = DouyinPageRuntime::new(&page)
        .ensure_douyin(true, 330.0)
        .await;
    Ok(dy_tools_with_llm_provider(page, Some(llm_provider)))
}

struct DouyinContentPlatform {
    tools: Vec<Arc<dyn Tool>>,
}

impl DouyinContentPlatform {
    fn tool(&self, operation: ContentOperation) -> anyhow::Result<&Arc<dyn Tool>> {
        self.tools
            .iter()
            .find(|tool| tool.name() == operation.tool_name())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Douyin content-platform tool is missing: {}",
                    operation.tool_name()
                )
            })
    }

    fn unsupported(operation: ContentOperation) -> ToolResult {
        json_result(&json!({
            "ok": false,
            "site": "dy",
            "reason": "unsupported_content_operation",
            "operation": operation.tool_name(),
        }))
    }
}

#[async_trait]
impl ContentPlatform for DouyinContentPlatform {
    fn site_id(&self) -> &'static str {
        "dy"
    }

    fn display_name(&self) -> &'static str {
        "Douyin"
    }

    fn capabilities(&self) -> ContentCapabilities {
        ContentCapabilities {
            full_search: false,
            author_full_scan: false,
            search_filters: false,
            comments: false,
            comment_replies: false,
            media_download: false,
            ocr: false,
            audio_transcription: false,
            cross_run_history: false,
            artifacts: false,
        }
    }

    fn supports_operation(&self, operation: ContentOperation) -> bool {
        matches!(
            operation,
            ContentOperation::Search | ContentOperation::PageState
        )
    }

    fn input_schema(&self, operation: ContentOperation) -> Value {
        match operation {
            ContentOperation::GetNotes => {
                get_notes_input_schema(video_note_locator_schema(), 8, false)
            }
            ContentOperation::Search => search_input_schema(empty_filters_schema(), 10, 5, false),
            ContentOperation::AuthorScan => author_scan_input_schema(5, false),
            ContentOperation::PageState => page_state_input_schema(),
        }
    }

    fn effective_input(&self, operation: ContentOperation, input: &Value) -> Value {
        let mut effective = input.clone();
        if operation == ContentOperation::Search && effective.get("num_notes").is_none() {
            effective["num_notes"] = json!(10);
        }
        effective
    }

    async fn get_notes(&self, _input: Value, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        Ok(Self::unsupported(ContentOperation::GetNotes))
    }

    async fn search(&self, mut input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        if let Some(num_notes) = input
            .as_object_mut()
            .and_then(|obj| obj.remove("num_notes"))
        {
            input["num"] = num_notes;
        }
        if let Some(object) = input.as_object_mut() {
            for key in [
                "filters",
                "num_comments",
                "download_media",
                "ocr",
                "transcribe_audio",
                "preview",
            ] {
                object.remove(key);
            }
        }
        self.tool(ContentOperation::Search)?.call(input, ctx).await
    }

    async fn author_scan(&self, _input: Value, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        Ok(Self::unsupported(ContentOperation::AuthorScan))
    }

    async fn page_state(&self, input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        self.tool(ContentOperation::PageState)?
            .call(input, ctx)
            .await
    }
}

pub fn dy_content_platform(
    page: Arc<PageSession>,
    _llm_provider: Option<Arc<dyn LlmProvider>>,
) -> Arc<dyn ContentPlatform> {
    Arc::new(DouyinContentPlatform {
        tools: dy_tools(page),
    })
}

pub fn dy_agent_instructions(extra: &str) -> String {
    let base = DY_KNOWLEDGE.trim().to_string();
    let extra = extra.trim();
    if extra.is_empty() {
        base
    } else {
        format!("{extra}\n\n{base}")
    }
}

pub static DY_SITE: SiteSpec = SiteSpec {
    id: "dy",
    about: "Douyin (douyin.com)",
    // Let Douyin tools own first navigation so they can use a much longer
    // timeout for the site's occasional 4-5 minute blank-page throttling.
    home_url: "",
    content_platform: dy_content_platform,
    agent_tools: |page, llm| Box::pin(dy_agent_tools(page, llm)),
    default_agent_tools: None,
    agent_instructions: dy_agent_instructions,
    default_agent_instructions: None,
    commands: &[
        SiteCommand {
            name: "search",
            tool_name: "search",
            about: "Search Douyin and print video result cards as JSON.",
            args: &[
                CommandArg {
                    key: "query",
                    long: None,
                    value_name: "QUERY",
                    help: "Search query",
                    required: true,
                    kind: ArgKind::Str,
                },
                CommandArg {
                    key: "num",
                    long: Some("num"),
                    value_name: "N",
                    help: "Number of video cards to collect by scrolling. Defaults to 10.",
                    required: false,
                    kind: ArgKind::Int,
                },
                CommandArg {
                    key: "wait_seconds",
                    long: Some("wait-seconds"),
                    value_name: "SECONDS",
                    help: "Maximum wait for page/search transitions. Use 300+ when Douyin web is throttled.",
                    required: false,
                    kind: ArgKind::Int,
                },
            ],
            slow: SlowWhen::Always,
            run: run_search,
        },
        SiteCommand {
            name: "page_state",
            tool_name: "page_state",
            about: "Open or reuse Douyin and print page state as JSON.",
            args: &[CommandArg {
                key: "wait_seconds",
                long: Some("wait-seconds"),
                value_name: "SECONDS",
                help: "Maximum wait for a non-blank Douyin page. Use 300+ when the web page is throttled.",
                required: false,
                kind: ArgKind::Int,
            }],
            slow: SlowWhen::Always,
            run: run_page_state,
        },
    ],
};

fn run_search(
    page: Arc<PageSession>,
    args: Value,
    debug_snapshot: bool,
    progress: Option<ToolProgressSender>,
) -> BoxFuture<Value> {
    Box::pin(async move {
        run_tool_command(
            ToolCommand {
                site_id: "dy",
                command_name: "search",
                tool_name: "search",
                before: None,
                after: None,
                include_run_metadata: false,
            },
            page.clone(),
            &dy_tools(page),
            args,
            debug_snapshot,
            progress,
        )
        .await
    })
}

fn run_page_state(
    page: Arc<PageSession>,
    args: Value,
    debug_snapshot: bool,
    progress: Option<ToolProgressSender>,
) -> BoxFuture<Value> {
    Box::pin(async move {
        let wait_seconds = get_f64(&args, "wait_seconds", 330.0);
        run_tool_command(
            ToolCommand {
                site_id: "dy",
                command_name: "page_state",
                tool_name: "page_state",
                before: Some(Box::new(move |page| {
                    Box::pin(async move {
                        let runtime = DouyinPageRuntime::new(&page);
                        runtime.ensure_douyin(true, wait_seconds).await?;
                        let _ = runtime.wait_until_interactive(wait_seconds).await?;
                        Ok(())
                    })
                })),
                after: None,
                include_run_metadata: false,
            },
            page.clone(),
            &dy_tools(page),
            args,
            debug_snapshot,
            progress,
        )
        .await
    })
}

pub struct SearchTool {
    page: Arc<PageSession>,
}

#[async_trait]
impl Tool for SearchTool {
    fn name(&self) -> &str {
        "search"
    }

    fn description(&self) -> &str {
        "Search Douyin for videos matching `query` and return visible result \
         cards (video id, URL, title, author, cover, and any engagement text \
         the page exposes). Defaults to 10 cards and may wait several minutes \
         if Douyin web is throttled."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "num": {
                    "type": "integer",
                    "description": "Number of video cards to collect by scrolling.",
                    "default": 10,
                    "minimum": 1
                },
                "wait_seconds": {
                    "type": "number",
                    "description": "Maximum wait for page/search transitions.",
                    "default": 330
                }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, input: Value, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let query = required_string(&input, "query")?;
        let wait_seconds = get_f64(&input, "wait_seconds", 330.0);
        let num_videos = get_i64(&input, "num", 10).max(1) as usize;
        let runtime = DouyinPageRuntime::new(&self.page);
        let value = runtime
            .search_videos(&query, wait_seconds, num_videos)
            .await?;
        Ok(json_result(&value))
    }
}

pub struct PageStateTool {
    page: Arc<PageSession>,
}

#[async_trait]
impl Tool for PageStateTool {
    fn name(&self) -> &str {
        "page_state"
    }

    fn description(&self) -> &str {
        "Read Douyin page state, including URL, title, candidate search inputs, \
         login hints, and whether the page still looks blank/throttled. This \
         may wait several minutes on Douyin web throttling."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "wait_seconds": {
                    "type": "number",
                    "description": "Maximum wait for the Douyin page to become non-blank.",
                    "default": 330
                }
            }
        })
    }

    async fn call(&self, input: Value, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let wait_seconds = get_f64(&input, "wait_seconds", 330.0);
        let runtime = DouyinPageRuntime::new(&self.page);
        let state = runtime.wait_until_interactive(wait_seconds).await?;
        Ok(json_result(&state))
    }
}
