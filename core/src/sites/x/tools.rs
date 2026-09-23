use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent::tool::ToolProgressSender;
use crate::agent::{Backend as LlmProvider, Tool, ToolContext, ToolResult};
use crate::cdp::PageSession;
use crate::sites::registry::{
    required_string, ArgKind, BoxFuture, CommandArg, NativeSiteAdapter, SiteCommand, SlowWhen,
};
use crate::sites::runner::{get_f64, get_i64, json_result, ToolCommand};
use crate::sites::skill_cli::{
    current_url, ensure_site_page, failure_payload, gate_reason, invoke_browser_tool,
    navigate_https, percent_encode_query, run_skill_command, wait_for_browser_tool,
    DEFAULT_COMMENT_COUNT, DEFAULT_RESULT_COUNT, DEFAULT_WAIT_SECONDS, MAX_TOOL_ITEMS,
    MAX_TOOL_WAIT_SECONDS,
};

const SITE_ID: &str = "x";
const HOME_URL: &str = "https://x.com/home";
const HOST_ROOT: &str = "x.com";
const RESERVED_PROFILE_NAMES: &[&str] = &[
    "compose",
    "explore",
    "home",
    "i",
    "intent",
    "login",
    "messages",
    "notifications",
    "search",
    "settings",
    "share",
    "signup",
];

pub async fn x_agent_tools(
    page: Arc<PageSession>,
    _llm_provider: Arc<dyn LlmProvider>,
) -> anyhow::Result<Vec<Arc<dyn Tool>>> {
    Ok(x_tools(page))
}

pub fn x_agent_instructions(extra: &str) -> String {
    crate::sites::learning::site_agent_instructions(SITE_ID, extra)
}

fn x_tools(page: Arc<PageSession>) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(SearchTool { page: page.clone() }),
        Arc::new(ProfileTool { page: page.clone() }),
        Arc::new(GetPostsTool { page: page.clone() }),
        Arc::new(ReplyTool { page: page.clone() }),
        Arc::new(PageStateTool { page }),
    ]
}

pub static X_NATIVE_ADAPTER: NativeSiteAdapter = NativeSiteAdapter {
    id: SITE_ID,
    about: "X (x.com)",
    home_url: "",
    agent_tools: |page, llm| Box::pin(x_agent_tools(page, llm)),
    default_agent_tools: None,
    agent_instructions: x_agent_instructions,
    default_agent_instructions: None,
    commands: &[
        SiteCommand {
            name: "search",
            tool_name: "search",
            about: "Search X and print structured post candidates as JSON.",
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
                    help: "Number of posts to collect by scrolling. Defaults to 10.",
                    required: false,
                    kind: ArgKind::Int,
                },
                CommandArg {
                    key: "wait_seconds",
                    long: Some("wait-seconds"),
                    value_name: "SECONDS",
                    help: "Maximum wait for the search page to hydrate. Defaults to 30.",
                    required: false,
                    kind: ArgKind::Int,
                },
            ],
            slow: SlowWhen::Always,
            run: run_search,
        },
        SiteCommand {
            name: "profile",
            tool_name: "profile",
            about: "Read an X profile and collect visible timeline posts.",
            args: &[
                CommandArg {
                    key: "profile",
                    long: None,
                    value_name: "HANDLE_OR_URL",
                    help: "X username or profile URL.",
                    required: true,
                    kind: ArgKind::Str,
                },
                CommandArg {
                    key: "num",
                    long: Some("num"),
                    value_name: "N",
                    help: "Number of visible posts to collect by scrolling. Defaults to 10.",
                    required: false,
                    kind: ArgKind::Int,
                },
                CommandArg {
                    key: "wait_seconds",
                    long: Some("wait-seconds"),
                    value_name: "SECONDS",
                    help: "Maximum wait for the profile to hydrate. Defaults to 30.",
                    required: false,
                    kind: ArgKind::Int,
                },
            ],
            slow: SlowWhen::Always,
            run: run_profile,
        },
        SiteCommand {
            name: "get-posts",
            tool_name: "get_posts",
            about: "Read X posts by URL or numeric id, including visible replies and media.",
            args: &[
                CommandArg {
                    key: "posts",
                    long: Some("post"),
                    value_name: "URL_OR_ID",
                    help: "X post URL or numeric id. Repeat to read multiple posts.",
                    required: true,
                    kind: ArgKind::StrList,
                },
                CommandArg {
                    key: "num_comments",
                    long: Some("num-comments"),
                    value_name: "N",
                    help: "Visible replies to collect per post. Defaults to 8; 0 skips replies.",
                    required: false,
                    kind: ArgKind::Int,
                },
                CommandArg {
                    key: "wait_seconds",
                    long: Some("wait-seconds"),
                    value_name: "SECONDS",
                    help: "Maximum wait for each post to hydrate. Defaults to 30.",
                    required: false,
                    kind: ArgKind::Int,
                },
            ],
            slow: SlowWhen::Always,
            run: run_get_posts,
        },
        SiteCommand {
            name: "reply",
            tool_name: "reply",
            about: "Reply to one X post using verified pointer and keyboard events.",
            args: &[
                CommandArg {
                    key: "post",
                    long: None,
                    value_name: "URL_OR_ID",
                    help: "Target X post URL or numeric id.",
                    required: true,
                    kind: ArgKind::Str,
                },
                CommandArg {
                    key: "text",
                    long: Some("text"),
                    value_name: "TEXT",
                    help: "Exact reply text. The command refuses to replace an existing draft.",
                    required: true,
                    kind: ArgKind::Str,
                },
                CommandArg {
                    key: "wait_seconds",
                    long: Some("wait-seconds"),
                    value_name: "SECONDS",
                    help:
                        "Maximum wait for hydration and post-submit reconciliation. Defaults to 30.",
                    required: false,
                    kind: ArgKind::Int,
                },
            ],
            slow: SlowWhen::Always,
            run: run_reply,
        },
        SiteCommand {
            name: "page_state",
            tool_name: "page_state",
            about: "Open or reuse X and print page, login, challenge, and rate-limit state.",
            args: &[CommandArg {
                key: "wait_seconds",
                long: Some("wait-seconds"),
                value_name: "SECONDS",
                help: "Maximum wait for X. Defaults to 30.",
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
    run_named(page, args, debug_snapshot, progress, "search", "search")
}

fn run_profile(
    page: Arc<PageSession>,
    args: Value,
    debug_snapshot: bool,
    progress: Option<ToolProgressSender>,
) -> BoxFuture<Value> {
    run_named(page, args, debug_snapshot, progress, "profile", "profile")
}

fn run_get_posts(
    page: Arc<PageSession>,
    args: Value,
    debug_snapshot: bool,
    progress: Option<ToolProgressSender>,
) -> BoxFuture<Value> {
    run_named(
        page,
        args,
        debug_snapshot,
        progress,
        "get-posts",
        "get_posts",
    )
}

fn run_page_state(
    page: Arc<PageSession>,
    args: Value,
    debug_snapshot: bool,
    progress: Option<ToolProgressSender>,
) -> BoxFuture<Value> {
    run_named(
        page,
        args,
        debug_snapshot,
        progress,
        "page_state",
        "page_state",
    )
}

fn run_reply(
    page: Arc<PageSession>,
    args: Value,
    debug_snapshot: bool,
    progress: Option<ToolProgressSender>,
) -> BoxFuture<Value> {
    run_named(page, args, debug_snapshot, progress, "reply", "reply")
}

fn run_named(
    page: Arc<PageSession>,
    args: Value,
    debug_snapshot: bool,
    progress: Option<ToolProgressSender>,
    command_name: &'static str,
    tool_name: &'static str,
) -> BoxFuture<Value> {
    run_skill_command(
        page.clone(),
        args,
        debug_snapshot,
        progress,
        ToolCommand {
            site_id: SITE_ID,
            command_name,
            tool_name,
            before: None,
            after: None,
            include_run_metadata: command_name == "get-posts",
        },
        x_tools(page),
    )
}

struct SearchTool {
    page: Arc<PageSession>,
}

#[async_trait]
impl Tool for SearchTool {
    fn name(&self) -> &str {
        "search"
    }

    fn description(&self) -> &str {
        "Search X for posts matching `query`. This tool is read-only."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "maxLength": 512 },
                "num": { "type": "integer", "default": 10, "minimum": 1, "maximum": 100 },
                "wait_seconds": { "type": "number", "default": 30, "minimum": 1, "maximum": 330 }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let query = required_string(&input, "query")?;
        if query.chars().count() > 512 {
            anyhow::bail!("query must contain at most 512 characters");
        }
        let num = get_i64(&input, "num", DEFAULT_RESULT_COUNT).clamp(1, MAX_TOOL_ITEMS);
        let wait_seconds =
            get_f64(&input, "wait_seconds", DEFAULT_WAIT_SECONDS).clamp(1.0, MAX_TOOL_WAIT_SECONDS);
        let target = format!(
            "https://x.com/search?q={}&src=typed_query&f=live",
            percent_encode_query(&query)
        );
        navigate_https(&self.page, &target).await?;
        let search_args = json!({ "query": query });
        let state = wait_for_browser_tool(
            &self.page,
            SITE_ID,
            "searchState",
            Some(&search_args),
            wait_seconds,
        )
        .await?;
        if let Some(reason) = gate_reason(&state) {
            return Ok(json_result(&failure_payload(
                reason,
                json!({ "query": query, "state": state, "count": 0, "results": [] }),
            )));
        }
        if state.get("ok").and_then(Value::as_bool) != Some(true) {
            return Ok(json_result(&failure_payload(
                state
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("search_results_unavailable"),
                json!({ "query": query, "state": state, "count": 0, "results": [] }),
            )));
        }
        let results = invoke_browser_tool(
            &self.page,
            ctx,
            SITE_ID,
            "searchResults",
            Some(&json!({ "limit": num })),
            true,
        )
        .await?;
        let final_state = crate::sites::learning::run_site_browser_tool(
            &self.page,
            SITE_ID,
            "searchState",
            Some(&search_args),
        )
        .await?;
        if final_state.get("ok").and_then(Value::as_bool) != Some(true) {
            let reason = gate_reason(&final_state).unwrap_or_else(|| {
                final_state
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("page_unavailable_after_scroll")
            });
            return Ok(json_result(&failure_payload(
                reason,
                json!({
                    "query": query,
                    "state": final_state,
                    "count": results.as_array().map(Vec::len).unwrap_or(0),
                    "results": results,
                    "partial": true,
                }),
            )));
        }
        Ok(json_result(&json!({
            "ok": true,
            "query": query,
            "url": current_url(&self.page).await.unwrap_or_default(),
            "count": results.as_array().map(Vec::len).unwrap_or(0),
            "results": results,
            "state": final_state,
        })))
    }
}

struct ProfileTool {
    page: Arc<PageSession>,
}

#[async_trait]
impl Tool for ProfileTool {
    fn name(&self) -> &str {
        "profile"
    }

    fn description(&self) -> &str {
        "Read an X profile and its visible posts by @handle or URL."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "profile": { "type": "string" },
                "num": { "type": "integer", "default": 10, "minimum": 1, "maximum": 100 },
                "wait_seconds": { "type": "number", "default": 30, "minimum": 1, "maximum": 330 }
            },
            "required": ["profile"]
        })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let locator = required_string(&input, "profile")?;
        let url = x_profile_url(&locator)?;
        let num = get_i64(&input, "num", DEFAULT_RESULT_COUNT).clamp(1, MAX_TOOL_ITEMS);
        let wait_seconds =
            get_f64(&input, "wait_seconds", DEFAULT_WAIT_SECONDS).clamp(1.0, MAX_TOOL_WAIT_SECONDS);
        navigate_https(&self.page, &url).await?;
        let state =
            wait_for_browser_tool(&self.page, SITE_ID, "profileDetail", None, wait_seconds).await?;
        if let Some(reason) = gate_reason(&state) {
            return Ok(json_result(&failure_payload(
                reason,
                json!({ "profile": locator, "url": url, "state": state }),
            )));
        }
        if state.get("ok").and_then(Value::as_bool) != Some(true) {
            return Ok(json_result(&failure_payload(
                state
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("profile_unavailable"),
                json!({ "profile": locator, "url": url, "state": state }),
            )));
        }
        let posts = invoke_browser_tool(
            &self.page,
            ctx,
            SITE_ID,
            "profilePosts",
            Some(&json!({ "limit": num })),
            true,
        )
        .await?;
        let final_state = crate::sites::learning::run_site_browser_tool(
            &self.page,
            SITE_ID,
            "profileDetail",
            None,
        )
        .await?;
        let expected_username = state
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let final_username = final_state
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if final_state.get("ok").and_then(Value::as_bool) != Some(true)
            || expected_username.is_empty()
            || final_username != expected_username
        {
            let reason = if final_state.get("ok").and_then(Value::as_bool) == Some(true) {
                "profile_changed_during_read"
            } else {
                gate_reason(&final_state).unwrap_or_else(|| {
                    final_state
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("page_unavailable_after_scroll")
                })
            };
            return Ok(json_result(&failure_payload(
                reason,
                json!({
                    "profile": state,
                    "state": final_state,
                    "posts": posts,
                    "partial": true,
                }),
            )));
        }
        Ok(json_result(&json!({
            "ok": true,
            "profile": state,
            "posts": posts,
            "count": posts.as_array().map(Vec::len).unwrap_or(0),
        })))
    }
}

struct GetPostsTool {
    page: Arc<PageSession>,
}

#[async_trait]
impl Tool for GetPostsTool {
    fn name(&self) -> &str {
        "get_posts"
    }

    fn description(&self) -> &str {
        "Read one or more X posts by URL or numeric id, including visible replies and media."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "posts": { "type": "array", "items": { "type": "string" } },
                "num_comments": { "type": "integer", "default": 8, "minimum": 0, "maximum": 100 },
                "wait_seconds": { "type": "number", "default": 30, "minimum": 1, "maximum": 330 }
            },
            "required": ["posts"]
        })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let posts = input
            .get("posts")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("missing required argument: posts"))?;
        if posts.is_empty() {
            anyhow::bail!("at least one --post is required");
        }
        let num_comments =
            get_i64(&input, "num_comments", DEFAULT_COMMENT_COUNT).clamp(0, MAX_TOOL_ITEMS);
        let wait_seconds =
            get_f64(&input, "wait_seconds", DEFAULT_WAIT_SECONDS).clamp(1.0, MAX_TOOL_WAIT_SECONDS);
        let mut items = Vec::new();
        let mut stopped_on_gate = None;
        for (index, post) in posts.iter().enumerate() {
            let locator = post
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("each --post must be a non-empty URL or id"))?;
            let item = read_x_post(&self.page, ctx, locator, num_comments, wait_seconds).await?;
            let gate = gate_reason(&item);
            items.push(item);
            if let Some(reason) = gate {
                stopped_on_gate = Some(reason);
                for remaining in &posts[index + 1..] {
                    items.push(json!({
                        "ok": false,
                        "status": "unprocessed_after_gate",
                        "reason": reason,
                        "input": remaining.as_str().unwrap_or_default(),
                    }));
                }
                break;
            }
        }
        Ok(json_result(&json!({
            "ok": items.iter().all(|item| item.get("ok").and_then(Value::as_bool) == Some(true)),
            "count": items.len(),
            "posts": items,
            "stopped_on_gate": stopped_on_gate,
        })))
    }
}

struct PageStateTool {
    page: Arc<PageSession>,
}

struct ReplyTool {
    page: Arc<PageSession>,
}

#[async_trait]
impl Tool for ReplyTool {
    fn name(&self) -> &str {
        "reply"
    }

    fn description(&self) -> &str {
        "Reply to an explicitly selected X post with real CDP pointer and keyboard events. Refuses ambiguous composers, existing drafts, route changes, and submit retries."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "post": { "type": "string" },
                "text": { "type": "string", "minLength": 1, "maxLength": 10000 },
                "wait_seconds": { "type": "number", "default": 30, "minimum": 1, "maximum": 330 }
            },
            "required": ["post", "text"]
        })
    }

    async fn call(&self, input: Value, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let locator = required_string(&input, "post")?;
        let raw_text = input
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing required argument: text"))?;
        if raw_text.trim().is_empty() || raw_text.trim() != raw_text {
            anyhow::bail!(
                "reply text must be non-empty and have no leading or trailing whitespace"
            );
        }
        let text = raw_text.to_string();
        if text.chars().count() > 10_000 {
            anyhow::bail!("reply text must contain at most 10000 characters");
        }
        let wait_seconds =
            get_f64(&input, "wait_seconds", DEFAULT_WAIT_SECONDS).clamp(1.0, MAX_TOOL_WAIT_SECONDS);
        let url = x_post_url(&locator)?;
        let expected_post_id = x_post_id(&url)
            .ok_or_else(|| anyhow::anyhow!("canonical X post URL is missing a status id"))?;
        navigate_https(&self.page, &url).await?;
        let detail =
            wait_for_browser_tool(&self.page, SITE_ID, "postDetail", None, wait_seconds).await?;
        if let Some(reason) = gate_reason(&detail) {
            return Ok(json_result(&failure_payload(
                reason,
                json!({ "post": locator, "url": url, "detail": detail, "submit_click_count": 0 }),
            )));
        }
        let post_id = detail.get("id").and_then(Value::as_str).unwrap_or_default();
        if detail.get("ok").and_then(Value::as_bool) != Some(true) || post_id != expected_post_id {
            return Ok(json_result(&failure_payload(
                if detail.get("ok").and_then(Value::as_bool) == Some(true) {
                    "wrong_post"
                } else {
                    detail
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("post_unavailable")
                },
                json!({ "post": locator, "expected_post_id": expected_post_id, "url": url, "detail": detail, "submit_click_count": 0 }),
            )));
        }
        let action_args = json!({ "post_id": post_id });
        let rendered_args = json!({ "post_id": post_id, "text": text });
        let before = crate::sites::learning::run_site_browser_tool(
            &self.page,
            SITE_ID,
            "renderedReplyState",
            Some(&rendered_args),
        )
        .await?;
        let baseline = before.get("count").and_then(Value::as_u64).unwrap_or(0);
        if before.get("visible").and_then(Value::as_bool) == Some(true) {
            return Ok(json_result(&json!({
                "ok": true,
                "status": "already_present",
                "post_id": post_id,
                "url": url,
                "reply": text,
                "submit_click_count": 0,
                "reconcile": before,
            })));
        }

        let editor = crate::sites::learning::run_site_browser_tool(
            &self.page,
            SITE_ID,
            "replyEditorTarget",
            Some(&action_args),
        )
        .await?;
        let Some((editor_x, editor_y)) = verified_write_target(&editor, post_id) else {
            return Ok(json_result(&failure_payload(
                editor
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("reply_editor_unavailable"),
                json!({ "post_id": post_id, "url": url, "editor": editor, "submit_click_count": 0 }),
            )));
        };
        self.page.click(editor_x, editor_y).await?;
        tokio::time::sleep(Duration::from_millis(150)).await;
        let draft = crate::sites::learning::run_site_browser_tool(
            &self.page,
            SITE_ID,
            "replyDraftState",
            Some(&action_args),
        )
        .await?;
        if draft.get("ok").and_then(Value::as_bool) != Some(true)
            || draft.get("focused").and_then(Value::as_bool) != Some(true)
            || !draft
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .is_empty()
        {
            return Ok(json_result(&failure_payload(
                "reply_editor_not_empty_or_focused",
                json!({ "post_id": post_id, "url": url, "draft": draft, "submit_click_count": 0 }),
            )));
        }
        self.page.type_chars(&text).await?;
        let typed_deadline = Instant::now() + Duration::from_secs(5);
        let typed = loop {
            let state = crate::sites::learning::run_site_browser_tool(
                &self.page,
                SITE_ID,
                "replyDraftState",
                Some(&action_args),
            )
            .await?;
            if state.get("value").and_then(Value::as_str) == Some(text.as_str())
                || Instant::now() >= typed_deadline
            {
                break state;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        };
        if typed.get("value").and_then(Value::as_str) != Some(text.as_str()) {
            return Ok(json_result(&failure_payload(
                "reply_draft_mismatch",
                json!({ "post_id": post_id, "url": url, "draft": typed, "submit_click_count": 0 }),
            )));
        }

        let final_page_state =
            crate::sites::learning::run_site_browser_tool(&self.page, SITE_ID, "pageState", None)
                .await?;
        let final_detail =
            crate::sites::learning::run_site_browser_tool(&self.page, SITE_ID, "postDetail", None)
                .await?;
        let final_draft = crate::sites::learning::run_site_browser_tool(
            &self.page,
            SITE_ID,
            "replyDraftState",
            Some(&action_args),
        )
        .await?;
        if final_page_state.get("ok").and_then(Value::as_bool) != Some(true)
            || final_detail.get("ok").and_then(Value::as_bool) != Some(true)
            || final_detail.get("id").and_then(Value::as_str) != Some(post_id)
            || final_draft.get("value").and_then(Value::as_str) != Some(text.as_str())
        {
            return Ok(json_result(&failure_payload(
                "volatile_state_changed_before_submit",
                json!({ "post_id": post_id, "url": url, "page_state": final_page_state, "detail": final_detail, "draft": final_draft, "submit_click_count": 0 }),
            )));
        }
        let submit = crate::sites::learning::run_site_browser_tool(
            &self.page,
            SITE_ID,
            "replySubmitTarget",
            Some(&action_args),
        )
        .await?;
        let Some((submit_x, submit_y)) = verified_write_target(&submit, post_id) else {
            return Ok(json_result(&failure_payload(
                submit
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("reply_submit_unavailable"),
                json!({ "post_id": post_id, "url": url, "submit": submit, "submit_click_count": 0 }),
            )));
        };

        let dispatch_error = self
            .page
            .click(submit_x, submit_y)
            .await
            .err()
            .map(|error| format!("{error:#}"));
        let deadline = Instant::now() + Duration::from_secs_f64(wait_seconds);
        let reconciled = loop {
            let state = crate::sites::learning::run_site_browser_tool(
                &self.page,
                SITE_ID,
                "renderedReplyState",
                Some(&rendered_args),
            )
            .await?;
            if state.get("count").and_then(Value::as_u64).unwrap_or(0) > baseline
                || Instant::now() >= deadline
            {
                break state;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        };
        let committed = reconciled.get("count").and_then(Value::as_u64).unwrap_or(0) > baseline;
        Ok(json_result(&json!({
            "ok": committed,
            "status": if committed { "committed" } else { "commit_unknown" },
            "post_id": post_id,
            "url": url,
            "reply": text,
            "interaction": "trusted_pointer_and_keyboard",
            "platform_api_called": false,
            "submit_click_count": 1,
            "dispatch_error": dispatch_error,
            "baseline_exact_reply_count": baseline,
            "reconcile": reconciled,
        })))
    }
}

fn verified_write_target(target: &Value, post_id: &str) -> Option<(f64, f64)> {
    if target.get("ok").and_then(Value::as_bool) != Some(true)
        || target.get("hit_owned").and_then(Value::as_bool) != Some(true)
        || target.get("post_id").and_then(Value::as_str) != Some(post_id)
    {
        return None;
    }
    Some((target.get("x")?.as_f64()?, target.get("y")?.as_f64()?))
}

#[async_trait]
impl Tool for PageStateTool {
    fn name(&self) -> &str {
        "page_state"
    }

    fn description(&self) -> &str {
        "Open or reuse X and return route, login, challenge, rate-limit, and hydration state."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "wait_seconds": { "type": "number", "default": 30, "minimum": 1, "maximum": 330 }
            }
        })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let wait_seconds =
            get_f64(&input, "wait_seconds", DEFAULT_WAIT_SECONDS).clamp(1.0, MAX_TOOL_WAIT_SECONDS);
        ensure_site_page(&self.page, HOST_ROOT, HOME_URL).await?;
        let _ = wait_for_browser_tool(&self.page, SITE_ID, "pageState", None, wait_seconds).await?;
        let state = invoke_browser_tool(&self.page, ctx, SITE_ID, "pageState", None, false).await?;
        Ok(json_result(&state))
    }
}

async fn read_x_post(
    page: &PageSession,
    ctx: &ToolContext,
    locator: &str,
    num_comments: i64,
    wait_seconds: f64,
) -> anyhow::Result<Value> {
    let url = x_post_url(locator)?;
    let expected_id = x_post_id(&url)
        .ok_or_else(|| anyhow::anyhow!("canonical X post URL is missing a status id"))?;
    navigate_https(page, &url).await?;
    let detail = wait_for_browser_tool(page, SITE_ID, "postDetail", None, wait_seconds).await?;
    if let Some(reason) = gate_reason(&detail) {
        return Ok(failure_payload(
            reason,
            json!({ "input": locator, "url": url, "entity": detail }),
        ));
    }
    if detail.get("ok").and_then(Value::as_bool) != Some(true)
        || detail.get("id").and_then(Value::as_str) != Some(expected_id.as_str())
    {
        return Ok(failure_payload(
            if detail.get("ok").and_then(Value::as_bool) == Some(true) {
                "wrong_post"
            } else {
                detail
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("post_unavailable")
            },
            json!({ "input": locator, "expected_id": expected_id, "url": url, "entity": detail }),
        ));
    }
    let entity = invoke_browser_tool(page, ctx, SITE_ID, "postDetail", None, false).await?;
    if let Some(reason) = gate_reason(&entity) {
        return Ok(failure_payload(
            reason,
            json!({ "input": locator, "url": url, "entity": entity }),
        ));
    }
    if entity.get("ok").and_then(Value::as_bool) != Some(true)
        || entity.get("id").and_then(Value::as_str) != Some(expected_id.as_str())
    {
        return Ok(failure_payload(
            if entity.get("ok").and_then(Value::as_bool) == Some(true) {
                "post_changed_during_read"
            } else {
                entity
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("post_changed_during_read")
            },
            json!({ "input": locator, "expected_id": expected_id, "url": url, "entity": entity }),
        ));
    }
    let comments = if num_comments > 0 {
        match invoke_browser_tool(
            page,
            ctx,
            SITE_ID,
            "comments",
            Some(&json!({ "limit": num_comments })),
            true,
        )
        .await
        {
            Ok(comments) => comments,
            Err(error) => {
                let state = crate::sites::learning::run_site_browser_tool(
                    page,
                    SITE_ID,
                    "pageState",
                    None,
                )
                .await
                .unwrap_or_else(|state_error| {
                    json!({ "ok": false, "status": "state_check_failed", "error": format!("{state_error:#}") })
                });
                let reason = gate_reason(&state).unwrap_or("comment_collection_failed");
                return Ok(failure_payload(
                    reason,
                    json!({
                        "input": locator,
                        "url": url,
                        "entity": entity,
                        "state": state,
                        "error": format!("{error:#}"),
                    }),
                ));
            }
        }
    } else {
        Value::Array(Vec::new())
    };
    let final_state =
        crate::sites::learning::run_site_browser_tool(page, SITE_ID, "postDetail", None).await?;
    let final_id = final_state
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if final_state.get("ok").and_then(Value::as_bool) != Some(true) || final_id != expected_id {
        let reason = if final_state.get("ok").and_then(Value::as_bool) == Some(true) {
            "post_changed_during_read"
        } else {
            gate_reason(&final_state).unwrap_or_else(|| {
                final_state
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("page_changed_during_read")
            })
        };
        return Ok(failure_payload(
            reason,
            json!({
                "input": locator,
                "url": url,
                "entity": entity,
                "comments": comments,
                "state": final_state,
                "partial": true,
            }),
        ));
    }
    Ok(json!({
        "ok": true,
        "input": locator,
        "url": current_url(page).await.unwrap_or(url),
        "entity": entity,
        "comments": comments,
        "state": final_state,
    }))
}

fn x_profile_url(locator: &str) -> anyhow::Result<String> {
    let trimmed = locator.trim();
    if let Ok(mut url) = reqwest::Url::parse(trimmed) {
        validate_x_url(&url, "profile")?;
        let parts = url
            .path_segments()
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.len() != 1 {
            anyhow::bail!("X profile URL must identify exactly one profile");
        }
        let username = parts[0].to_ascii_lowercase();
        validate_username(&username, locator)?;
        url.set_host(Some("x.com"))?;
        url.set_path(&format!("/{username}"));
        url.set_query(None);
        url.set_fragment(None);
        return Ok(url.to_string());
    }
    let username = trimmed.trim_start_matches('@').to_ascii_lowercase();
    validate_username(&username, locator)?;
    Ok(format!("https://x.com/{username}"))
}

fn x_post_url(locator: &str) -> anyhow::Result<String> {
    let trimmed = locator.trim();
    if let Ok(mut url) = reqwest::Url::parse(trimmed) {
        validate_x_url(&url, "post")?;
        let identity = url
            .path_segments()
            .map(|segments| segments.collect::<Vec<_>>())
            .and_then(|parts| match parts.as_slice() {
                [username, "status", id]
                    if valid_username_segment(username)
                        && !id.is_empty()
                        && id.chars().all(|character| character.is_ascii_digit()) =>
                {
                    Some((username.to_ascii_lowercase(), (*id).to_string()))
                }
                ["i", "web", "status", id]
                    if !id.is_empty() && id.chars().all(|character| character.is_ascii_digit()) =>
                {
                    Some(("i/web".to_string(), (*id).to_string()))
                }
                _ => None,
            });
        let (owner, id) =
            identity.ok_or_else(|| anyhow::anyhow!("invalid X post URL: {locator}"))?;
        url.set_host(Some("x.com"))?;
        url.set_path(&format!("/{owner}/status/{id}"));
        url.set_query(None);
        url.set_fragment(None);
        return Ok(url.to_string());
    }
    if trimmed.len() < 5 || !trimmed.chars().all(|character| character.is_ascii_digit()) {
        anyhow::bail!("invalid X post URL or numeric id: {locator}");
    }
    Ok(format!("https://x.com/i/web/status/{trimmed}"))
}

fn x_post_id(raw_url: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw_url).ok()?;
    let parts = url
        .path_segments()?
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let id = match parts.as_slice() {
        [_username, "status", id] => *id,
        ["i", "web", "status", id] => *id,
        _ => return None,
    };
    (!id.is_empty() && id.chars().all(|character| character.is_ascii_digit()))
        .then(|| id.to_string())
}

fn validate_x_url(url: &reqwest::Url, kind: &str) -> anyhow::Result<()> {
    if url.scheme() != "https" {
        anyhow::bail!("X {kind} URL must be https");
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if !matches!(
        host.as_str(),
        "x.com" | "www.x.com" | "twitter.com" | "www.twitter.com"
    ) {
        anyhow::bail!("X {kind} URL must use x.com or twitter.com");
    }
    if !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("X {kind} URL must not contain credentials");
    }
    Ok(())
}

fn validate_username(username: &str, locator: &str) -> anyhow::Result<()> {
    if username.is_empty()
        || username.len() > 15
        || !username
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        || RESERVED_PROFILE_NAMES.contains(&username)
    {
        anyhow::bail!("invalid X username or profile URL: {locator}");
    }
    Ok(())
}

fn valid_username_segment(username: &str) -> bool {
    !username.is_empty()
        && username.len() <= 15
        && username
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        && !RESERVED_PROFILE_NAMES.contains(&username.to_ascii_lowercase().as_str())
}
