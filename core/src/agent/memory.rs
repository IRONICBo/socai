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
const TURN_MARKDOWN_MAX_CHARS: usize = 2_000;
const USER_REQUEST_MAX_CHARS: usize = 500;
const ENTITY_FACT_VALUE_MAX_CHARS: usize = 160;
const ENTITY_FACTS_MAX: usize = 12;
const ENTITY_SUMMARY_MAX_CHARS: usize = 360;
const COMPACT_CONTEXT_HEADING: &str = "# Earlier compacted context";
const LEGACY_EVIDENCE_HEADING: &str = "# Earlier tool evidence";

#[derive(Debug, Default)]
struct CompactEntityEvidence {
    title: String,
    evidence_id: Option<String>,
    facts: BTreeMap<String, String>,
}

type ArtifactEvidence = BTreeMap<String, BTreeMap<String, CompactEntityEvidence>>;

/// Rewrite the transcript only when it has grown beyond `compact_after` full
/// messages. The message at `anchor_user_index` — the current run's user task,
/// index `0` for a fresh run and `seed_messages.len()` for a follow-up — stays
/// verbatim at the front; the last `keep_recent` messages remain verbatim
/// (widened backward when the window would open on a tool_result message, so
/// tool_use/tool_result pairs never split); older tool outputs become artifact
/// locators. Follow-up runs (`is_follow_up`) also get a short task
/// reminder adjacent to the recent tail. `anchor_user_index` is updated to the
/// anchor's new position after a rewrite so a later compaction in the same run
/// cannot accidentally pin a tool message; `is_follow_up` remains stable so
/// every later rewrite can recreate the reminder. Mutating the transcript,
/// rather than rebuilding a summary for every request, leaves the request
/// prefix stable until the next sawtooth compaction point and therefore
/// friendly to provider prompt caches.
pub fn compact_messages_for_context(
    messages: &mut Vec<Message>,
    compact_after: usize,
    keep_recent: usize,
    anchor_user_index: &mut usize,
    is_follow_up: bool,
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
    let anchor_idx = (*anchor_user_index).min(messages.len().saturating_sub(1));
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

    let task_reminder = is_follow_up
        .then(|| user_text(&anchor))
        .flatten()
        .map(|task| current_task_reminder(&task));
    let recent_ends_with_assistant = recent
        .last()
        .is_some_and(|message| matches!(message.role, MessageRole::Assistant));

    let mut compacted = Vec::with_capacity(3 + recent.len());
    compacted.push(anchor);
    if !evidence.is_empty() {
        compacted.push(Message::user(evidence));
    }
    if !recent_ends_with_assistant {
        compacted.extend(task_reminder.clone());
    }
    compacted.extend(recent);
    if recent_ends_with_assistant {
        compacted.extend(task_reminder);
    }
    *messages = compacted;
    *anchor_user_index = 0;
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
    let mut inherited = Vec::new();
    let mut evidence_refs: BTreeMap<String, String> = BTreeMap::new();
    let mut artifacts: ArtifactEvidence = BTreeMap::new();
    let mut turns = Vec::new();
    let mut pending_user: Option<String> = None;

    for message in messages {
        match (&message.role, &message.content) {
            (MessageRole::User, MessageContent::Text(text)) => {
                if text.starts_with(COMPACT_CONTEXT_HEADING)
                    || text.starts_with(LEGACY_EVIDENCE_HEADING)
                {
                    inherited.push(text.trim().to_string());
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
            let parsed: Vec<Value> = content
                .iter()
                .filter_map(|item| match item {
                    ToolResultContent::Text { text } => serde_json::from_str(text).ok(),
                    _ => None,
                })
                .collect();
            let result_evidence_id = parsed
                .iter()
                .find_map(|value| collect_evidence_reference(value, &mut evidence_refs));
            for value in &parsed {
                collect_evidence_reference(value, &mut evidence_refs);
                collect_artifact_evidence(value, result_evidence_id.as_deref(), &mut artifacts);
            }
        }
    }

    if let Some(user) = pending_user.take().filter(|text| !text.trim().is_empty()) {
        turns.push(compact_turn_markdown(
            Some(user.as_str()),
            "(no assistant report was recorded before compaction)",
        ));
    }

    let mut rendered = if inherited.is_empty() {
        COMPACT_CONTEXT_HEADING.to_string()
    } else {
        inherited.join("\n\n")
    };
    if !turns.is_empty() {
        rendered.push_str("\n\n## Earlier conversation turns\n");
        for (index, turn) in turns.iter().enumerate() {
            rendered.push_str(&format!("\n### Turn {}\n{}", index + 1, turn));
        }
    }
    if !evidence_refs.is_empty() {
        rendered.push_str("\n\n## Evidence references from earlier tool results\n");
        rendered.push_str(
            "Use these short IDs in submit_research_completion.evidence_refs; do not reconstruct locators.\n",
        );
        for (id, locator) in evidence_refs {
            rendered.push_str(&format!("- {id} = {locator}\n"));
        }
    }
    if !artifacts.is_empty() {
        rendered.push_str("\n\n## Earlier tool evidence\n");
        rendered.push_str("Full data is available in the listed artifacts.\n");
        for (path, entities) in artifacts {
            rendered.push_str(&format!("\n## Artifact: {path}\n"));
            for (id, entity) in entities {
                rendered.push_str(&render_entity_evidence(&id, &entity));
                rendered.push('\n');
            }
        }
    }
    rendered
}

fn collect_evidence_reference(
    value: &Value,
    evidence_refs: &mut BTreeMap<String, String>,
) -> Option<String> {
    let id = value
        .get("evidence_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())?;
    let locator = value
        .get("canonical_locator")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|locator| !locator.is_empty())?;
    evidence_refs.insert(id.to_string(), locator.to_string());
    Some(id.to_string())
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
        rendered.push_str(&truncate_chars(user, USER_REQUEST_MAX_CHARS));
        rendered.push_str("\n\n");
    }
    rendered.push_str("Assistant report excerpt:\n");
    rendered.push_str(&truncate_chars(markdown, TURN_MARKDOWN_MAX_CHARS));

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

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars).collect();
    out.push_str("\n\n[truncated; full report remains in the run artifact]");
    out
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
    evidence_id: Option<&str>,
    artifacts: &mut ArtifactEvidence,
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
        let profile = value.get("profile").and_then(Value::as_object);
        let title = profile
            .and_then(|profile| {
                ["nickname", "display_name", "name", "title"]
                    .iter()
                    .find_map(|key| profile.get(*key).and_then(Value::as_str))
            })
            .unwrap_or("");
        let mut facts = profile.map(compact_scalar_facts).unwrap_or_default();
        if let Some(date) = value
            .pointer("/notes/0/entity/date")
            .or_else(|| value.pointer("/notes/0/date"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|date| !date.is_empty())
        {
            facts
                .entry("latest_note_date".to_string())
                .or_insert_with(|| truncate_chars(date, ENTITY_FACT_VALUE_MAX_CHARS));
        }
        let entity = entities.entry(format!("author:{author_id}")).or_default();
        merge_entity_evidence(entity, title, evidence_id, facts);
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
                let entity = entities.entry(key).or_default();
                merge_entity_evidence(entity, title, evidence_id, BTreeMap::new());
            }
        }
    }
}

fn compact_scalar_facts(profile: &serde_json::Map<String, Value>) -> BTreeMap<String, String> {
    let mut facts: Vec<(String, String)> = profile
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "nickname" | "display_name" | "name" | "title"))
        .filter_map(|(key, value)| scalar_fact_value(value).map(|value| (key.clone(), value)))
        .collect();
    facts.sort_by(|(left_key, left_value), (right_key, right_value)| {
        left_value
            .chars()
            .count()
            .cmp(&right_value.chars().count())
            .then_with(|| left_key.cmp(right_key))
    });
    facts.truncate(ENTITY_FACTS_MAX);
    facts.into_iter().collect()
}

fn scalar_fact_value(value: &Value) -> Option<String> {
    let raw = match value {
        Value::String(value) => value.split_whitespace().collect::<Vec<_>>().join(" "),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null | Value::Array(_) | Value::Object(_) => return None,
    };
    (!raw.is_empty()).then(|| truncate_chars(&raw, ENTITY_FACT_VALUE_MAX_CHARS))
}

fn merge_entity_evidence(
    entity: &mut CompactEntityEvidence,
    title: &str,
    evidence_id: Option<&str>,
    facts: BTreeMap<String, String>,
) {
    if entity.title.is_empty() && !title.trim().is_empty() {
        entity.title = title.trim().to_string();
    }
    if entity.evidence_id.is_none() {
        entity.evidence_id = evidence_id.map(ToOwned::to_owned);
    }
    for (key, value) in facts {
        entity.facts.entry(key).or_insert(value);
    }
}

fn render_entity_evidence(id: &str, entity: &CompactEntityEvidence) -> String {
    let identity = match &entity.evidence_id {
        Some(evidence_id) => format!("{evidence_id} / {id}"),
        None => id.to_string(),
    };
    let mut rendered = if entity.title.is_empty() {
        format!("- {identity}")
    } else {
        format!("- {identity} — {}", entity.title)
    };
    let mut facts: Vec<(&String, &String)> = entity.facts.iter().collect();
    facts.sort_by(|(left_key, left_value), (right_key, right_value)| {
        left_value
            .chars()
            .count()
            .cmp(&right_value.chars().count())
            .then_with(|| left_key.cmp(right_key))
    });
    for (key, value) in facts {
        let fragment = format!("; {key}={value}");
        if rendered.chars().count() + fragment.chars().count() > ENTITY_SUMMARY_MAX_CHARS {
            break;
        }
        rendered.push_str(&fragment);
    }
    rendered
}
