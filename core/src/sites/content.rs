//! XHS-shaped content-platform contract shared by research sites.
//!
//! Xiaohongshu is the reference implementation: platform adapters expose the
//! same four content operations and keep site-specific page/runtime details
//! behind this trait. Callers select an implementation by `site_id` instead of
//! branching on Douyin, TikTok, or XHS tool names.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};

use crate::agent::{Backend as LlmProvider, Tool, ToolContext, ToolResult};
use crate::cdp::PageSession;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentOperation {
    GetNotes,
    Search,
    AuthorScan,
    PageState,
}

impl ContentOperation {
    pub const fn tool_name(self) -> &'static str {
        match self {
            Self::GetNotes => "get_notes",
            Self::Search => "search",
            Self::AuthorScan => "author_scan",
            Self::PageState => "page_state",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ContentCapabilities {
    pub full_search: bool,
    pub author_full_scan: bool,
    pub search_filters: bool,
    pub comments: bool,
    pub comment_replies: bool,
    pub media_download: bool,
    pub ocr: bool,
    pub audio_transcription: bool,
    pub cross_run_history: bool,
    pub artifacts: bool,
}

/// The stable content interface. Its operation names and request vocabulary
/// follow the established XHS tools: content items are `notes`, result limits
/// are `num_notes`, and full scans can request comments/media/OCR/ASR.
#[async_trait]
pub trait ContentPlatform: Send + Sync {
    fn site_id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn capabilities(&self) -> ContentCapabilities;
    fn input_schema(&self, operation: ContentOperation) -> Value;

    fn effective_input(&self, _operation: ContentOperation, input: &Value) -> Value {
        input.clone()
    }

    async fn get_notes(&self, input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult>;
    async fn search(&self, input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult>;
    async fn author_scan(&self, input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult>;
    async fn page_state(&self, input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult>;
}

struct ContentPlatformTool {
    platform: Arc<dyn ContentPlatform>,
    operation: ContentOperation,
    description: String,
}

impl ContentPlatformTool {
    fn new(platform: Arc<dyn ContentPlatform>, operation: ContentOperation) -> Self {
        let site = platform.display_name();
        let description = match operation {
            ContentOperation::GetNotes => format!(
                "Read one or more {site} notes by the locators returned from search or author_scan."
            ),
            ContentOperation::Search => format!(
                "Research {site} by keyword. The default is an XHS-style full scan; preview=true returns note cards only."
            ),
            ContentOperation::AuthorScan => format!(
                "Research one {site} author and their notes. The default is a full scan; preview=true returns note cards only."
            ),
            ContentOperation::PageState => {
                format!("Read the current {site} page, login, and modal state.")
            }
        };
        Self {
            platform,
            operation,
            description,
        }
    }
}

#[async_trait]
impl Tool for ContentPlatformTool {
    fn name(&self) -> &str {
        self.operation.tool_name()
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        self.platform.input_schema(self.operation)
    }

    fn defer_until_site(&self) -> &str {
        self.platform.site_id()
    }

    fn effective_input(&self, input: &Value) -> Value {
        self.platform.effective_input(self.operation, input)
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        match self.operation {
            ContentOperation::GetNotes => self.platform.get_notes(input, ctx).await,
            ContentOperation::Search => self.platform.search(input, ctx).await,
            ContentOperation::AuthorScan => self.platform.author_scan(input, ctx).await,
            ContentOperation::PageState => self.platform.page_state(input, ctx).await,
        }
    }
}

/// Build the common four-tool surface for any selected platform adapter.
pub fn content_platform_tools(platform: Arc<dyn ContentPlatform>) -> Vec<Arc<dyn Tool>> {
    [
        ContentOperation::GetNotes,
        ContentOperation::Search,
        ContentOperation::AuthorScan,
        ContentOperation::PageState,
    ]
    .into_iter()
    .map(|operation| {
        Arc::new(ContentPlatformTool::new(platform.clone(), operation)) as Arc<dyn Tool>
    })
    .collect()
}

/// Select the site implementation through the existing registry so adding a
/// platform does not require another dispatch branch in the shared contract.
pub fn select_content_platform(
    site_id: &str,
    page: Arc<PageSession>,
    llm_provider: Option<Arc<dyn LlmProvider>>,
) -> anyhow::Result<Arc<dyn ContentPlatform>> {
    let site = crate::sites::find_site(site_id)
        .ok_or_else(|| anyhow::anyhow!("unknown content platform: {site_id}"))?;
    Ok((site.content_platform)(page, llm_provider))
}

pub fn empty_filters_schema() -> Value {
    json!({
        "type": "object",
        "description": "Platform search filters. This implementation currently accepts no filter keys.",
        "properties": {},
        "additionalProperties": false
    })
}

pub fn tokenized_note_locator_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "note_id": { "type": "string" },
            "xsec_token": { "type": "string" }
        },
        "required": ["note_id", "xsec_token"],
        "additionalProperties": false
    })
}

pub fn video_note_locator_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "note_id": { "type": "string", "description": "Platform video/content id." },
            "url": { "type": "string", "description": "Canonical content URL when available." }
        },
        "required": ["note_id"],
        "additionalProperties": false
    })
}

pub fn get_notes_input_schema(
    locator_schema: Value,
    default_comments: i64,
    asr_enabled: bool,
) -> Value {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "notes": {
                "type": "array",
                "description": "Note locators previously returned by search or author_scan.",
                "minItems": 1,
                "maxItems": 20,
                "items": locator_schema
            },
            "num_comments": {
                "type": "integer",
                "description": "Comments to load per note; replies count toward the total. 0 skips comments.",
                "default": default_comments,
                "minimum": 0
            },
            "download_media": {
                "type": "boolean",
                "description": "Download note images/videos into the run dir and include local paths.",
                "default": false
            },
            "ocr": {
                "type": "boolean",
                "description": "Run local OCR on note images or a video note's cover.",
                "default": false
            },
            "transcribe_audio": {
                "type": "boolean",
                "description": "For video notes, download the video and transcribe audio while signed in with socai agent selected.",
                "default": false
            }
        },
        "required": ["notes"],
        "additionalProperties": false
    });
    if !asr_enabled {
        strip_hosted_transcription_schema(&mut schema);
    }
    schema
}

pub fn search_input_schema(
    filters_schema: Value,
    default_notes: i64,
    default_comments: i64,
    asr_enabled: bool,
) -> Value {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "query": { "type": "string" },
            "filters": filters_schema,
            "num_notes": {
                "type": "integer",
                "description": "Number of notes to collect and read. In preview mode, the number of cards to collect.",
                "default": default_notes,
                "minimum": 1
            },
            "num_comments": {
                "type": "integer",
                "description": "Comments to load per note. Ignored in preview mode.",
                "default": default_comments,
                "minimum": 0
            },
            "download_media": {
                "type": "boolean",
                "description": "Download note images/videos into the run dir and include local paths. Ignored in preview mode.",
                "default": false
            },
            "ocr": {
                "type": "boolean",
                "description": "Run local OCR on note images or video covers.",
                "default": false
            },
            "transcribe_audio": {
                "type": "boolean",
                "description": "For opened video notes, download the video and transcribe audio while signed in with socai agent selected. Ignored in preview mode.",
                "default": false
            },
            "preview": {
                "type": "boolean",
                "description": "Fast cards-only mode without opening notes or reading bodies/comments.",
                "default": false
            }
        },
        "required": ["query"]
    });
    if !asr_enabled {
        strip_hosted_transcription_schema(&mut schema);
    }
    schema
}

pub fn author_scan_input_schema(default_comments: i64, asr_enabled: bool) -> Value {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "author_id": { "type": "string", "description": "Platform author id or profile URL." },
            "num_notes": { "type": "integer", "minimum": 1 },
            "num_comments": {
                "type": "integer",
                "description": "Comments to load per note. Ignored in preview mode.",
                "default": default_comments,
                "minimum": 0
            },
            "preview": {
                "type": "boolean",
                "description": "Fast cards-only mode without opening notes.",
                "default": false
            },
            "download_media": {
                "type": "boolean",
                "description": "Download note images/videos and include local paths. Ignored in preview mode.",
                "default": false
            },
            "ocr": {
                "type": "boolean",
                "description": "Run local OCR on note images or video covers.",
                "default": false
            },
            "transcribe_audio": {
                "type": "boolean",
                "description": "For opened video notes, download the video and transcribe audio while signed in with socai agent selected. Ignored in preview mode.",
                "default": false
            }
        },
        "required": ["author_id"]
    });
    if !asr_enabled {
        strip_hosted_transcription_schema(&mut schema);
    }
    schema
}

pub fn page_state_input_schema() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

/// Apply the established XHS app/TUI defaults to every platform adapter.
/// Preview scans OCR covers in memory; full scans also retain downloaded media.
pub fn xhs_product_effective_input(operation: ContentOperation, input: &Value) -> Value {
    let mut effective = input.clone();
    if operation == ContentOperation::Search && effective.get("num_notes").is_none() {
        effective["num_notes"] = json!(10);
    }
    if operation == ContentOperation::PageState {
        return effective;
    }

    effective["ocr"] = json!(true);
    let preview = effective
        .get("preview")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if preview {
        if let Some(object) = effective.as_object_mut() {
            object.remove("download_media");
            object.remove("transcribe_audio");
        }
    } else {
        effective["download_media"] = json!(true);
    }
    effective
}

pub fn strip_hosted_transcription_schema(schema: &mut Value) {
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.remove("transcribe_audio");
    }
}

pub fn strip_hosted_transcription_input(input: &mut Value) -> bool {
    input
        .as_object_mut()
        .and_then(|object| object.remove("transcribe_audio"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}
