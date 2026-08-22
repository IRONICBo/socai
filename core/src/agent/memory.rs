//! Deterministic, artifact-first context compaction for long agent runs.
//!
//! Keep a growing tail of full messages so the provider can reuse its prompt
//! cache. Once that tail reaches its limit, replace only the older tool
//! results with durable evidence locators (post/author id, title, artifact
//! path), then start growing the tail again.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::agent::llm::{Block, Message, MessageContent, MessageRole, ToolResultContent};

pub const DEFAULT_COMPACT_AFTER_MESSAGES: usize = 20;
pub const DEFAULT_KEEP_RECENT_MESSAGES: usize = 10;
const TURN_MARKDOWN_MAX_CHARS: usize = 3_000;
const USER_REQUEST_MAX_CHARS: usize = 1_000;
const COMPACT_CONTEXT_MAX_CHARS: usize = 48_000;
const COMPACT_TURNS_MAX_CHARS: usize = 24_000;
const COMPACT_CONTEXT_HEADING: &str = "# Earlier compacted context";
const LEGACY_EVIDENCE_HEADING: &str = "# Earlier tool evidence";

/// Rewrite the transcript only when it has grown beyond `compact_after` full
/// messages. The message at `anchor_user_index` — the current run's user task,
/// index `0` for a fresh run and `seed_messages.len()` for a follow-up — stays
/// verbatim at the front; the last `keep_recent` messages remain verbatim
/// (widened backward when the window would open on a tool_result message, so
/// tool_use/tool_result pairs never split); older tool outputs become artifact
/// locators. Follow-up runs (`anchor_user_index > 0`) also get a short task
/// reminder immediately before the recent tail. Mutating the transcript, rather
/// than rebuilding a summary for every request, leaves the request prefix
/// stable until the next sawtooth compaction point and therefore friendly to
/// provider prompt caches.
pub fn compact_messages_for_context(
    messages: &mut Vec<Message>,
    compact_after: usize,
    keep_recent: usize,
    anchor_user_index: usize,
) -> bool {
    if compact_after == 0
        || keep_recent == 0
        || keep_recent >= compact_after
        || messages.len() <= compact_after
    {
        return false;
    }

    let mut recent_start = messages.len() - keep_recent;
    // Tool results live in a user message appended immediately after the
    // assistant message carrying the matching tool_use blocks, and providers
    // reject a request that keeps one side of that pair without the other
    // (OpenAI-compat: tool message without tool_calls; Anthropic: tool_result
    // without its tool_use). A count-based boundary can land between the two —
    // an extra lone user message (max-tokens discard note, forced-summary
    // prompt) shifts the window onto the tool_result — so widen the window
    // until it no longer starts mid-pair.
    while recent_start > 1 && contains_tool_result(&messages[recent_start]) {
        recent_start -= 1;
    }
    let anchor_idx = anchor_user_index.min(messages.len().saturating_sub(1));
    let anchor = messages[anchor_idx].clone();
    let older: Vec<Message> = (0..recent_start)
        .filter(|&index| index != anchor_idx)
        .map(|index| messages[index].clone())
        .collect();
    let recent: Vec<Message> = messages[recent_start..]
        .iter()
        .enumerate()
        .filter(|(offset, _)| recent_start + offset != anchor_idx)
        .map(|(_, message)| message.clone())
        .collect();
    let evidence = compact_older_messages(&older);

    let mut compacted = Vec::with_capacity(3 + recent.len());
    compacted.push(anchor);
    if !evidence.is_empty() {
        compacted.push(Message::user(evidence));
    }
    if anchor_user_index > 0 {
        if let Some(task) = user_text(&compacted[0]) {
            compacted.push(current_task_reminder(&task));
        }
    }
    compacted.extend(recent);
    *messages = compacted;
    true
}

fn user_text(message: &Message) -> Option<String> {
    if !matches!(message.role, MessageRole::User) {
        return None;
    }
    match &message.content {
        MessageContent::Text(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        MessageContent::Blocks(_) => None,
    }
}

fn current_task_reminder(task: &str) -> Message {
    Message::user(format!(
        "Current task (do not confuse with earlier turns): {task}"
    ))
}

fn contains_tool_result(message: &Message) -> bool {
    match &message.content {
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .any(|block| matches!(block, Block::ToolResult { .. })),
        MessageContent::Text(_) => false,
    }
}

fn compact_older_messages(messages: &[Message]) -> String {
    let mut inherited_artifacts: BTreeMap<String, BTreeMap<String, CompactEntityEvidence>> =
        BTreeMap::new();
    let mut artifacts: BTreeMap<String, BTreeMap<String, CompactEntityEvidence>> = BTreeMap::new();
    let mut turns = Vec::new();
    let mut pending_user: Option<String> = None;

    for message in messages {
        match (&message.role, &message.content) {
            (MessageRole::User, MessageContent::Text(text)) => {
                if text.starts_with(COMPACT_CONTEXT_HEADING)
                    || text.starts_with(LEGACY_EVIDENCE_HEADING)
                {
                    collect_inherited_context(text, &mut turns, &mut inherited_artifacts);
                } else {
                    pending_user = Some(text.trim().to_string());
                }
                continue;
            }
            (MessageRole::Assistant, _) => {
                if let Some(markdown) = assistant_report_markdown(message) {
                    turns.push(compact_turn_markdown(
                        pending_user.take().as_deref(),
                        &markdown,
                    ));
                    continue;
                }
            }
            _ => {}
        }

        let MessageContent::Blocks(blocks) = &message.content else {
            continue;
        };
        for block in blocks {
            let Block::ToolResult { content, .. } = block else {
                continue;
            };
            for item in content {
                let ToolResultContent::Text { text } = item else {
                    continue;
                };
                let Ok(value) = serde_json::from_str::<Value>(text) else {
                    continue;
                };
                collect_artifact_evidence(&value, &mut artifacts);
            }
        }
    }

    if let Some(user) = pending_user.take().filter(|text| !text.trim().is_empty()) {
        turns.push(compact_turn_markdown(
            Some(user.as_str()),
            "(no assistant report was recorded before compaction)",
        ));
    }

    for (path, entities) in artifacts {
        let inherited = inherited_artifacts.entry(path).or_default();
        for entity in entities.values() {
            inherited
                .entry(entity.key())
                .and_modify(|existing| existing.merge_from(entity))
                .or_insert_with(|| entity.clone());
        }
    }
    render_bounded_context(&turns, &inherited_artifacts)
}

fn collect_inherited_context(
    text: &str,
    turns: &mut Vec<String>,
    artifacts: &mut BTreeMap<String, BTreeMap<String, CompactEntityEvidence>>,
) {
    enum Section {
        None,
        Turns,
        Evidence,
    }
    let mut section = Section::None;
    let mut current_turn = String::new();
    let mut current_artifact: Option<String> = None;
    let mut inside_pre = false;
    let flush_turn = |turns: &mut Vec<String>, current: &mut String| {
        let turn = current.trim();
        if !turn.is_empty() && !turns.iter().any(|existing| existing == turn) {
            turns.push(turn.to_string());
        }
        current.clear();
    };

    for line in text.lines() {
        let structural = !inside_pre;
        if structural && line == "## Earlier conversation turns" {
            flush_turn(turns, &mut current_turn);
            section = Section::Turns;
            current_artifact = None;
            continue;
        }
        if structural && (line == "## Earlier tool evidence" || line == LEGACY_EVIDENCE_HEADING) {
            flush_turn(turns, &mut current_turn);
            section = Section::Evidence;
            current_artifact = None;
            continue;
        }
        if structural && matches!(section, Section::Turns) && line.starts_with("### Turn ") {
            flush_turn(turns, &mut current_turn);
            continue;
        }
        if structural && matches!(section, Section::Evidence) {
            if let Some(path) = line.strip_prefix("## Artifact: ") {
                current_artifact = Some(path.trim().to_string());
                artifacts.entry(path.trim().to_string()).or_default();
                continue;
            }
            if let (Some(path), Some(evidence)) =
                (current_artifact.as_ref(), line.strip_prefix("- "))
            {
                let candidate = CompactEntityEvidence::parse(evidence.trim());
                artifacts
                    .entry(path.clone())
                    .or_default()
                    .entry(candidate.key())
                    .and_modify(|existing| existing.merge_from(&candidate))
                    .or_insert(candidate);
            }
            continue;
        }
        if matches!(section, Section::Turns) {
            current_turn.push_str(line);
            current_turn.push('\n');
        }
        if line.contains("<pre>") && !line.contains("</pre>") {
            inside_pre = true;
        }
        if line.contains("</pre>") {
            inside_pre = false;
        }
    }
    flush_turn(turns, &mut current_turn);
}

fn render_bounded_context(
    turns: &[String],
    artifacts: &BTreeMap<String, BTreeMap<String, CompactEntityEvidence>>,
) -> String {
    let mut rendered = COMPACT_CONTEXT_HEADING.to_string();
    if !turns.is_empty() {
        let mut selected = Vec::new();
        let mut used = 0usize;
        for turn in turns.iter().rev() {
            let block_chars = turn.chars().count() + 32;
            if used + block_chars <= COMPACT_TURNS_MAX_CHARS {
                selected.push(turn);
                used += block_chars;
            }
        }
        selected.reverse();
        rendered.push_str("\n\n## Earlier conversation turns\n");
        let omitted = turns.len().saturating_sub(selected.len());
        if omitted > 0 {
            rendered.push_str(&format!("\n[{omitted} older compacted turns omitted]\n"));
        }
        for (index, turn) in selected.into_iter().enumerate() {
            let block = format!("\n### Turn {}\n{}", index + 1, turn);
            if rendered.chars().count() + block.chars().count() > COMPACT_CONTEXT_MAX_CHARS {
                break;
            }
            rendered.push_str(&block);
        }
    }
    if !artifacts.is_empty() {
        let heading =
            "\n\n## Earlier tool evidence\nFull data is available in the listed artifacts.\n";
        if rendered.chars().count() + heading.chars().count() <= COMPACT_CONTEXT_MAX_CHARS {
            rendered.push_str(heading);
        }
        let mut omitted_entities = 0usize;
        for (path, entities) in artifacts {
            let artifact_heading = format!("\n## Artifact: {path}\n");
            if rendered.chars().count() + artifact_heading.chars().count()
                > COMPACT_CONTEXT_MAX_CHARS
            {
                omitted_entities += entities.len();
                continue;
            }
            rendered.push_str(&artifact_heading);
            for entity in entities.values() {
                let line = format!("- {}\n", entity.render());
                if rendered.chars().count() + line.chars().count() > COMPACT_CONTEXT_MAX_CHARS {
                    omitted_entities += 1;
                } else {
                    rendered.push_str(&line);
                }
            }
        }
        if omitted_entities > 0 {
            let marker = format!(
                "\n[{omitted_entities} evidence entries omitted; use the artifact paths above]\n"
            );
            if rendered.chars().count() + marker.chars().count() <= COMPACT_CONTEXT_MAX_CHARS {
                rendered.push_str(&marker);
            }
        }
    }
    rendered
}

fn assistant_report_markdown(message: &Message) -> Option<String> {
    if !matches!(message.role, MessageRole::Assistant) {
        return None;
    }
    match &message.content {
        MessageContent::Text(text) => (!text.trim().is_empty()).then(|| text.trim().to_string()),
        MessageContent::Blocks(blocks) => {
            if blocks
                .iter()
                .any(|block| !matches!(block, Block::Text { .. }))
            {
                return None;
            }
            let markdown = blocks
                .iter()
                .filter_map(|block| match block {
                    Block::Text { text } => Some(text.trim()),
                    _ => None,
                })
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            (!markdown.is_empty()).then_some(markdown)
        }
    }
}

fn compact_turn_markdown(user: Option<&str>, markdown: &str) -> String {
    let mut rendered = String::new();
    if let Some(user) = user.filter(|text| !text.trim().is_empty()) {
        rendered.push_str("User request:\n");
        rendered.push_str("<pre>");
        rendered.push_str(&escape_html_text(&compact_head_tail(
            user,
            USER_REQUEST_MAX_CHARS,
            700,
            "[middle omitted; full request remains in the conversation record]",
        )));
        rendered.push_str("</pre>\n\n");
    }
    rendered.push_str("Assistant report excerpt:\n");
    rendered.push_str("<pre>");
    rendered.push_str(&escape_html_text(&compact_head_tail(
        markdown,
        TURN_MARKDOWN_MAX_CHARS,
        2_000,
        "[middle omitted; full report remains in the run artifact]",
    )));
    rendered.push_str("</pre>");

    let (notes, artifacts) = extract_markdown_evidence(markdown);
    if !notes.is_empty() || !artifacts.is_empty() {
        rendered.push_str("\n\nExtracted evidence:\n");
        for (id, title) in notes {
            if title.is_empty() {
                rendered.push_str(&format!("- note_id: {id}\n"));
            } else {
                rendered.push_str(&format!("- note_id: {id}; title: {title}\n"));
            }
        }
        for path in artifacts {
            rendered.push_str(&format!("- artifact: {path}\n"));
        }
    }
    rendered
}

fn compact_head_tail(text: &str, max_chars: usize, head_chars: usize, marker: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let separator = format!("\n\n{marker}\n\n");
    let separator_chars = separator.chars().count();
    if max_chars <= separator_chars {
        return separator.chars().take(max_chars).collect();
    }
    let content_chars = max_chars - separator_chars;
    let head_chars = head_chars.min(content_chars);
    let tail_chars = content_chars.saturating_sub(head_chars);
    let head: String = trimmed.chars().take(head_chars).collect();
    let tail: String = trimmed
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}{separator}{tail}")
}

fn escape_html_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn extract_markdown_evidence(markdown: &str) -> (BTreeSet<(String, String)>, BTreeSet<String>) {
    let mut notes = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    let mut cursor = 0;

    while let Some(open_offset) = markdown[cursor..].find('[') {
        let open = cursor + open_offset;
        let Some(close_offset) = markdown[open + 1..].find(']') else {
            break;
        };
        let close = open + 1 + close_offset;
        if markdown.as_bytes().get(close + 1) != Some(&b'(') {
            cursor = close + 1;
            continue;
        }
        let target_start = close + 2;
        let Some(target_offset) = markdown[target_start..].find(')') else {
            break;
        };
        let target_end = target_start + target_offset;
        let title = markdown[open + 1..close].trim();
        let target = markdown[target_start..target_end].trim();

        if let Some(note_id) = target.strip_prefix("note:") {
            let note_id = note_id.trim();
            if !note_id.is_empty() {
                notes.insert((note_id.to_string(), title.to_string()));
            }
        } else if is_artifact_link(target) {
            artifacts.insert(target.to_string());
        }
        cursor = target_end + 1;
    }

    (notes, artifacts)
}

fn is_artifact_link(target: &str) -> bool {
    let normalized = target.replace('\\', "/");
    normalized.contains("/.socai/runs/")
        || normalized.starts_with("artifacts/")
        || normalized.contains("/artifacts/")
        || normalized.starts_with("tools/")
        || normalized.contains("/tools/")
        || normalized.starts_with("snapshots/")
        || normalized.contains("/snapshots/")
        || normalized.starts_with("site_media/")
        || normalized.contains("/site_media/")
}

fn collect_artifact_evidence(
    value: &Value,
    artifacts: &mut BTreeMap<String, BTreeMap<String, CompactEntityEvidence>>,
) {
    let Some(path) = value
        .pointer("/artifact/path")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
    else {
        return;
    };
    let entities = artifacts.entry(path.to_string()).or_default();

    if let Some(author_id) = value.get("author_id").and_then(Value::as_str) {
        let title = value
            .pointer("/profile/nickname")
            .or_else(|| value.pointer("/profile/display_name"))
            .or_else(|| value.pointer("/profile/name"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let id = format!("author:{author_id}");
        let candidate = CompactEntityEvidence {
            id,
            title: title.to_string(),
            ..CompactEntityEvidence::default()
        };
        entities
            .entry(candidate.id.clone())
            .and_modify(|existing| existing.merge_from(&candidate))
            .or_insert(candidate);
    }

    for key in ["notes", "cards"] {
        let Some(items) = value.get(key).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let entity = item.get("entity").unwrap_or(item);
            let id = entity
                .get("note_id")
                .or_else(|| entity.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let title = entity.get("title").and_then(Value::as_str).unwrap_or("");
            if !id.is_empty() || !title.is_empty() {
                let key = if id.is_empty() {
                    format!("title:{title}")
                } else {
                    id.to_string()
                };
                let candidate = CompactEntityEvidence {
                    id: id.to_string(),
                    title: title.to_string(),
                    author: string_field(entity, "author"),
                    author_id: string_field(entity, "author_id"),
                    url: string_field(entity, "url"),
                    date: string_field(entity, "date"),
                    note_type: string_field(entity, "type"),
                    likes: scalar_field(entity, "likes"),
                    favorites: scalar_field(entity, "favorites"),
                    comments_count: scalar_field(entity, "comments_count"),
                };
                entities
                    .entry(key)
                    .and_modify(|existing| existing.merge_from(&candidate))
                    .or_insert(candidate);
            }
        }
    }
}

#[derive(Clone, Default)]
struct CompactEntityEvidence {
    id: String,
    title: String,
    author: String,
    author_id: String,
    url: String,
    date: String,
    note_type: String,
    likes: String,
    favorites: String,
    comments_count: String,
}

impl CompactEntityEvidence {
    fn parse(rendered: &str) -> Self {
        let (main, metadata) = rendered
            .split_once(" | ")
            .map_or((rendered, ""), |(main, metadata)| (main, metadata));
        let (id, title) = main
            .split_once(" — ")
            .map_or((main.trim(), ""), |(id, title)| (id.trim(), title.trim()));
        let mut parsed = Self {
            id: id.to_string(),
            title: title.to_string(),
            ..Self::default()
        };
        for field in metadata.split("; ") {
            let Some((label, value)) = field.split_once(": ") else {
                continue;
            };
            match label {
                "author" => parsed.author = value.to_string(),
                "author_id" => parsed.author_id = value.to_string(),
                "date" => parsed.date = value.to_string(),
                "type" => parsed.note_type = value.to_string(),
                "likes" => parsed.likes = value.to_string(),
                "favorites" => parsed.favorites = value.to_string(),
                "comments" => parsed.comments_count = value.to_string(),
                "url" => parsed.url = value.to_string(),
                _ => {}
            }
        }
        parsed
    }

    fn key(&self) -> String {
        if self.id.trim().is_empty() {
            format!("title:{}", self.title.trim())
        } else {
            self.id.trim().to_string()
        }
    }

    fn merge_from(&mut self, other: &Self) {
        merge_field(&mut self.id, &other.id);
        merge_field(&mut self.title, &other.title);
        merge_field(&mut self.author, &other.author);
        merge_field(&mut self.author_id, &other.author_id);
        merge_field(&mut self.url, &other.url);
        merge_field(&mut self.date, &other.date);
        merge_field(&mut self.note_type, &other.note_type);
        merge_field(&mut self.likes, &other.likes);
        merge_field(&mut self.favorites, &other.favorites);
        merge_field(&mut self.comments_count, &other.comments_count);
    }

    fn render(&self) -> String {
        let mut main = match (self.id.is_empty(), self.title.is_empty()) {
            (false, false) => format!("{} — {}", self.id, compact_field(&self.title, 120)),
            (false, true) => self.id.clone(),
            (true, false) => compact_field(&self.title, 120),
            (true, true) => "unknown note".to_string(),
        };
        let mut metadata = Vec::new();
        push_metadata(&mut metadata, "author", &self.author, 80);
        push_metadata(&mut metadata, "author_id", &self.author_id, 80);
        push_metadata(&mut metadata, "date", &self.date, 40);
        push_metadata(&mut metadata, "type", &self.note_type, 30);
        push_metadata(&mut metadata, "likes", &self.likes, 30);
        push_metadata(&mut metadata, "favorites", &self.favorites, 30);
        push_metadata(&mut metadata, "comments", &self.comments_count, 30);
        push_metadata(&mut metadata, "url", &self.url, 220);
        if !metadata.is_empty() {
            main.push_str(" | ");
            main.push_str(&metadata.join("; "));
        }
        main
    }
}

fn merge_field(current: &mut String, candidate: &str) {
    if candidate.trim().is_empty() {
        return;
    }
    if current.trim().is_empty() || candidate.chars().count() > current.chars().count() {
        *current = candidate.to_string();
    }
}

fn string_field(entity: &Value, key: &str) -> String {
    entity
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn scalar_field(entity: &Value, key: &str) -> String {
    match entity.get(key) {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        _ => String::new(),
    }
}

fn push_metadata(metadata: &mut Vec<String>, label: &str, value: &str, max_chars: usize) {
    if !value.trim().is_empty() {
        metadata.push(format!("{label}: {}", compact_field(value, max_chars)));
    }
}

fn compact_field(value: &str, max_chars: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut compact: String = value.chars().take(max_chars).collect();
    compact.push('…');
    compact
}
