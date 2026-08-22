//! Shared text/JSON compaction helpers used by agent history and run-state
//! evidence summaries.
//!
//! These are used in two places:
//! - Agent loop history (keep tool_result bodies bounded so context doesn't
//!   blow up over many steps).
//! - RunState (compact entity-like payloads when summarizing
//!   evidence for working_memory.md).

use serde_json::{Map, Value};

/// Sized so a full multi-note scan (10 notes × ~1000-char body + OCR +
/// comments ≈ 25k chars) passes through uncompacted — "give me N notes with
/// full text" is the product's core ask, and compaction below can only
/// degrade it.
pub const TOOL_RESULT_TEXT_MAX_CHARS: usize = 30_000;
pub const ASSISTANT_TEXT_MAX_CHARS: usize = 320;

/// Generic cap for string leaves inside a compacted JSON tool result.
const COMPACT_STRING_MAX_CHARS: usize = 320;
/// Keys whose string values carry a post's body text. They keep more text
/// than other strings so the agent can still quote a note's full content
/// after compaction (XHS bodies max out at 1000 chars).
const BODY_TEXT_KEYS: &[&str] = &["content"];
const BODY_TEXT_MAX_CHARS: usize = 2_000;
/// Arrays inside a compacted JSON result keep this many items; a dropped
/// tail is replaced with a marker naming the omitted count so the model
/// reports "list was cut" instead of inventing "there were only 5".
const COMPACT_ARRAY_MAX_ITEMS: usize = 5;
/// Per-note cap applied to `ocr_text` when a result is over budget. OCR text
/// alone can dwarf every note body in a scan (it grows with image count, so
/// no overall budget outruns it); capping instead of dropping keeps the gist
/// of image-heavy posts readable.
const OCR_TEXT_MAX_CHARS: usize = 1_000;
/// Enrichment key stripped after the OCR cap when still over budget.
const STRIPPED_ENRICHMENT_KEY: &str = "top_comments";
/// What a stripped enrichment value is replaced with.
const ENRICHMENT_OMITTED_MARKER: &str = "[omitted to fit context; full data in the run artifact]";

const XHS_NOTE_METADATA_FIELDS: &[&str] = &[
    "note_id",
    "url",
    "title",
    "author",
    "author_id",
    "date",
    "date_edited",
    "type",
    "likes",
    "favorites",
    "comments_count",
    "location",
    "ip_location",
    "hashtags",
];

const XHS_PROFILE_FIELDS: &[&str] = &[
    "display_name",
    "nickname",
    "name",
    "xhs_id",
    "url",
    "bio",
    "ip_location",
    "verified",
    "verification",
    "followers",
    "following",
    "likes_and_collections",
];

/// Trim a string to at most `max_chars` characters, suffixing
/// `... [truncated]` when the original was longer. Char-based, not byte-based,
/// to keep UTF-8 safe.
pub fn truncate(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let count = trimmed.chars().count();
    if count <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars).collect();
    out.push_str("... [truncated]");
    out
}

/// Like [`truncate`] but tailored for tool_result bodies (longer ceiling,
/// "..." suffix on its own line).
pub fn truncate_result(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    const MARKER: &str = "\n... [truncated]";
    let marker_chars = MARKER.chars().count();
    if max_chars <= marker_chars {
        return MARKER.chars().take(max_chars).collect();
    }
    let mut out: String = text.chars().take(max_chars - marker_chars).collect();
    out.push_str(MARKER);
    out
}

/// Reorder + truncate a JSON value so the most "interesting" keys come
/// first and total size stays bounded. Body-text fields ([`BODY_TEXT_KEYS`])
/// keep up to [`BODY_TEXT_MAX_CHARS`]; every other string is capped at
/// [`COMPACT_STRING_MAX_CHARS`].
pub fn compact_json_value(value: &Value) -> Value {
    compact_json_value_with_body_cap(value, BODY_TEXT_MAX_CHARS)
}

fn compact_json_value_with_body_cap(value: &Value, body_cap: usize) -> Value {
    let preferred = [
        "ok",
        "error",
        "message",
        "site",
        "action",
        "entity_type",
        "query",
        "count",
        "state",
        "result",
        "cards",
        "entity",
        "title",
        "url",
        "summary",
    ];
    match value {
        Value::Object(map) => {
            let mut ordered_keys: Vec<String> = preferred
                .iter()
                .filter_map(|p| {
                    if map.contains_key(*p) {
                        Some((*p).to_string())
                    } else {
                        None
                    }
                })
                .collect();
            for key in map.keys() {
                if !ordered_keys.iter().any(|k| k == key) {
                    ordered_keys.push(key.clone());
                }
            }
            let mut out = Map::new();
            for key in ordered_keys.iter().take(16) {
                if let Some(v) = map.get(key) {
                    let compacted = match v {
                        Value::String(s) if BODY_TEXT_KEYS.contains(&key.as_str()) => {
                            Value::String(truncate(s, body_cap))
                        }
                        other => compact_json_value_with_body_cap(other, body_cap),
                    };
                    out.insert(key.clone(), compacted);
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => {
            let mut head: Vec<Value> = arr
                .iter()
                .take(COMPACT_ARRAY_MAX_ITEMS)
                .map(|v| compact_json_value_with_body_cap(v, body_cap))
                .collect();
            if arr.len() > COMPACT_ARRAY_MAX_ITEMS {
                head.push(Value::String(format!(
                    "... [{} more items truncated; full list in the run artifact]",
                    arr.len() - COMPACT_ARRAY_MAX_ITEMS
                )));
            }
            Value::Array(head)
        }
        Value::String(s) => Value::String(truncate(s, COMPACT_STRING_MAX_CHARS)),
        other => other.clone(),
    }
}

/// Bound a tool-result text. If it parses as JSON and is too long, degrade
/// in order of what the agent can best afford to lose:
/// 1. cap each note's `ocr_text` at [`OCR_TEXT_MAX_CHARS`], then strip
///    comment threads, so every note's body/link stays intact,
/// 2. [`compact_json_value`] keeping body text at its larger cap,
/// 3. recompact with the flat string cap,
/// 4. tail-chop as the last resort.
///
/// Otherwise truncate the string directly.
pub fn compress_text_maybe_json(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if let Ok(mut value) = serde_json::from_str::<Value>(text) {
        // XHS search/author results carry a durable artifact pointer and a
        // potentially large note list. Compact every note into a metadata-first
        // record before the generic array limiter can reduce the list to five.
        // A small comment/reply sample keeps common question-answer evidence;
        // OCR and media remain in the artifact.
        let xhs_result = is_xhs_artifact_result(&value);
        for (level, body_cap, comment_limit) in [
            (XhsMetadataLevel::Full, 700, 3),
            (XhsMetadataLevel::Full, 320, 1),
            (XhsMetadataLevel::Full, 0, 0),
            (XhsMetadataLevel::Identity, 0, 0),
            (XhsMetadataLevel::IdOnly, 0, 0),
        ] {
            if let Some(compact) = compact_xhs_tool_result(&value, level, body_cap, comment_limit) {
                if let Ok(rendered) = serde_json::to_string(&compact) {
                    if rendered.chars().count() <= max_chars {
                        return rendered;
                    }
                }
            }
        }
        if xhs_result {
            return compact_xhs_id_index_to_budget(&value, max_chars);
        }
        if cap_string_key_deep(&mut value, "ocr_text", OCR_TEXT_MAX_CHARS) {
            if let Ok(rendered) = serde_json::to_string(&value) {
                if rendered.chars().count() <= max_chars {
                    return rendered;
                }
            }
        }
        if strip_key_deep(&mut value, STRIPPED_ENRICHMENT_KEY) {
            if let Ok(rendered) = serde_json::to_string(&value) {
                if rendered.chars().count() <= max_chars {
                    return rendered;
                }
            }
        }
        let compact = compact_json_value(&value);
        if let Ok(rendered) = serde_json::to_string_pretty(&compact) {
            if rendered.chars().count() <= max_chars {
                return rendered;
            }
        }
        let flat = compact_json_value_with_body_cap(&value, COMPACT_STRING_MAX_CHARS);
        if let Ok(rendered) = serde_json::to_string_pretty(&flat) {
            return truncate_result(&rendered, max_chars);
        }
    }
    truncate_result(text, max_chars)
}

#[derive(Clone, Copy)]
enum XhsMetadataLevel {
    Full,
    Identity,
    IdOnly,
}

fn is_xhs_artifact_result(value: &Value) -> bool {
    value
        .pointer("/artifact/path")
        .and_then(Value::as_str)
        .is_some_and(|path| !path.trim().is_empty())
        && value.as_object().is_some_and(|source| {
            source.get("notes").is_some_and(Value::is_array)
                || source.get("cards").is_some_and(Value::is_array)
        })
}

fn compact_xhs_tool_result(
    value: &Value,
    level: XhsMetadataLevel,
    body_cap: usize,
    comment_limit: usize,
) -> Option<Value> {
    let source = value.as_object()?;
    let artifact_path = value.pointer("/artifact/path")?.as_str()?.trim();
    if !is_xhs_artifact_result(value) {
        return None;
    }

    let mut compact = Map::new();
    for key in [
        "ok",
        "reason",
        "error",
        "query",
        "author_id",
        "search_feedback",
    ] {
        if let Some(field) = source.get(key) {
            compact.insert(key.to_string(), compact_xhs_field(field, 320));
        }
    }
    compact.insert(
        "artifact".into(),
        json_object([("path", Value::String(artifact_path.to_string()))]),
    );
    if let Some(profile) = source.get("profile").and_then(Value::as_object) {
        compact.insert(
            "profile".into(),
            Value::Object(compact_selected_fields(profile, XHS_PROFILE_FIELDS)),
        );
    }
    for key in ["notes", "cards"] {
        let Some(items) = source.get(key).and_then(Value::as_array) else {
            continue;
        };
        compact.insert(
            key.to_string(),
            Value::Array(
                items
                    .iter()
                    .filter_map(|item| compact_xhs_note(item, level, body_cap, comment_limit))
                    .collect(),
            ),
        );
    }
    compact.insert(
        "context_compaction".into(),
        json_object([
            (
                "note",
                Value::String(
                    "Post bodies and comment threads were compacted; full note data, OCR, and media metadata remain in the artifact."
                        .to_string(),
                ),
            ),
            ("artifact_path", Value::String(artifact_path.to_string())),
        ]),
    );
    Some(Value::Object(compact))
}

fn compact_xhs_note(
    item: &Value,
    level: XhsMetadataLevel,
    body_cap: usize,
    comment_limit: usize,
) -> Option<Value> {
    let wrapped = item.get("entity").is_some();
    let entity = item.get("entity").unwrap_or(item).as_object()?;
    let fields = match level {
        XhsMetadataLevel::Full => XHS_NOTE_METADATA_FIELDS,
        XhsMetadataLevel::Identity => &[
            "note_id",
            "url",
            "title",
            "author",
            "author_id",
            "date",
            "type",
        ],
        XhsMetadataLevel::IdOnly => &["note_id"],
    };
    let mut compact = compact_selected_fields(entity, fields);
    if matches!(level, XhsMetadataLevel::Full) && body_cap > 0 {
        if let Some(content) = entity
            .get("content")
            .and_then(Value::as_str)
            .filter(|content| !content.is_empty())
        {
            compact.insert("content".into(), Value::String(truncate(content, body_cap)));
        }
    }
    if matches!(level, XhsMetadataLevel::Full) && comment_limit > 0 {
        if let Some(comments) = entity.get("top_comments").and_then(Value::as_array) {
            let comments = comments
                .iter()
                .take(comment_limit)
                .filter_map(compact_xhs_comment)
                .collect::<Vec<_>>();
            if !comments.is_empty() {
                compact.insert("top_comments".into(), Value::Array(comments));
            }
        }
    }
    if compact.is_empty() {
        return None;
    }
    let compact = Value::Object(compact);
    if wrapped {
        Some(json_object([("entity", compact)]))
    } else {
        Some(compact)
    }
}

fn compact_xhs_comment(comment: &Value) -> Option<Value> {
    if let Some(text) = comment.as_str() {
        return Some(Value::String(truncate(text, 240)));
    }
    let source = comment.as_object()?;
    let mut compact = Map::new();
    for key in ["text", "author", "is_author", "likes", "time"] {
        let Some(value) = source.get(key) else {
            continue;
        };
        compact.insert(
            key.to_string(),
            match value {
                Value::String(text) => Value::String(truncate(text, 240)),
                other => other.clone(),
            },
        );
    }
    for key in ["sub_comments", "replies"] {
        let Some(replies) = source.get(key).and_then(Value::as_array) else {
            continue;
        };
        let replies = replies
            .iter()
            .take(2)
            .filter_map(compact_xhs_comment)
            .collect::<Vec<_>>();
        if !replies.is_empty() {
            compact.insert(key.to_string(), Value::Array(replies));
        }
    }
    (!compact.is_empty()).then_some(Value::Object(compact))
}

fn compact_selected_fields(source: &Map<String, Value>, fields: &[&str]) -> Map<String, Value> {
    let mut compact = Map::new();
    for key in fields {
        if let Some(value) = source.get(*key) {
            compact.insert((*key).to_string(), compact_xhs_field(value, 320));
        }
    }
    compact
}

fn compact_xhs_field(value: &Value, string_cap: usize) -> Value {
    match value {
        Value::String(text) => Value::String(truncate_exact(text, string_cap)),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .take(12)
                .map(|item| compact_xhs_field(item, 80))
                .collect(),
        ),
        Value::Object(_) => compact_json_value_with_body_cap(value, string_cap),
        other => other.clone(),
    }
}

fn truncate_exact(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let mut compact: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    compact.push('…');
    compact
}

fn compact_xhs_id_index_to_budget(value: &Value, max_chars: usize) -> String {
    let artifact_path = value
        .pointer("/artifact/path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut compact = Map::new();
    compact.insert(
        "artifact".into(),
        json_object([("path", Value::String(truncate_exact(artifact_path, 1_000)))]),
    );
    let total = ["notes", "cards"]
        .into_iter()
        .filter_map(|key| value.get(key).and_then(Value::as_array))
        .map(Vec::len)
        .sum::<usize>();
    compact.insert("total_items".into(), Value::from(total));
    compact.insert("retained_items".into(), Value::from(0));
    compact.insert("omitted_items".into(), Value::from(total));
    compact.insert(
        "context_compaction".into(),
        Value::String(
            "Metadata is indexed by note_id; full records remain in the artifact.".into(),
        ),
    );

    let mut retained = 0usize;
    for key in ["notes", "cards"] {
        let Some(items) = value.get(key).and_then(Value::as_array) else {
            continue;
        };
        let mut ids = Vec::new();
        for item in items {
            let entity = item.get("entity").unwrap_or(item);
            let Some(note_id) = entity
                .get("note_id")
                .or_else(|| entity.get("id"))
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
            else {
                continue;
            };
            ids.push(Value::String(truncate_exact(note_id, 96)));
            compact.insert(key.into(), Value::Array(ids.clone()));
            compact.insert("retained_items".into(), Value::from(retained + 1));
            compact.insert(
                "omitted_items".into(),
                Value::from(total.saturating_sub(retained + 1)),
            );
            let fits = serde_json::to_string(&compact)
                .is_ok_and(|rendered| rendered.chars().count() <= max_chars);
            if !fits {
                ids.pop();
                compact.insert(key.into(), Value::Array(ids));
                break;
            }
            retained += 1;
        }
    }
    compact.insert("retained_items".into(), Value::from(retained));
    compact.insert(
        "omitted_items".into(),
        Value::from(total.saturating_sub(retained)),
    );
    serde_json::to_string(&compact)
        .ok()
        .filter(|rendered| rendered.chars().count() <= max_chars)
        .unwrap_or_else(|| {
            if max_chars >= 2 {
                "{}".to_string()
            } else {
                String::new()
            }
        })
}

fn json_object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

/// Cap every occurrence of `key` (at any depth) to `max_chars` total. A
/// string value is truncated; an array of strings keeps whole items until
/// the budget runs out, truncating the item that crosses it and dropping the
/// rest (with a trailing `... [truncated]` marker so the cut is visible).
/// Returns whether anything was shortened.
fn cap_string_key_deep(value: &mut Value, key: &str, max_chars: usize) -> bool {
    match value {
        Value::Object(map) => {
            let mut capped = false;
            for (k, v) in map.iter_mut() {
                if k == key {
                    capped |= cap_text_value(v, max_chars);
                } else {
                    capped |= cap_string_key_deep(v, key, max_chars);
                }
            }
            capped
        }
        Value::Array(arr) => {
            let mut capped = false;
            for v in arr.iter_mut() {
                capped |= cap_string_key_deep(v, key, max_chars);
            }
            capped
        }
        _ => false,
    }
}

/// Shorten one string or string-array value to at most `max_chars` characters
/// in total. Returns whether it was shortened.
fn cap_text_value(value: &mut Value, max_chars: usize) -> bool {
    match value {
        Value::String(s) => {
            if s.chars().count() <= max_chars {
                return false;
            }
            *value = Value::String(truncate(s, max_chars));
            true
        }
        Value::Array(arr) => {
            let mut budget = max_chars;
            let mut kept: Vec<Value> = Vec::new();
            let mut cut = false;
            for item in arr.iter() {
                let Some(s) = item.as_str() else {
                    kept.push(item.clone());
                    continue;
                };
                if cut {
                    continue;
                }
                let len = s.chars().count();
                if len <= budget {
                    budget -= len;
                    kept.push(item.clone());
                } else {
                    cut = true;
                    if budget > 0 {
                        // truncate() appends its own "... [truncated]" marker.
                        kept.push(Value::String(truncate(s, budget)));
                    } else {
                        kept.push(Value::String("... [truncated]".to_string()));
                    }
                }
            }
            if cut {
                *value = Value::Array(kept);
            }
            cut
        }
        _ => false,
    }
}

/// Replace every occurrence of `key` (at any depth) with
/// [`ENRICHMENT_OMITTED_MARKER`]. Returns whether anything was replaced.
fn strip_key_deep(value: &mut Value, key: &str) -> bool {
    match value {
        Value::Object(map) => {
            let mut stripped = false;
            for (k, v) in map.iter_mut() {
                if k == key {
                    *v = Value::String(ENRICHMENT_OMITTED_MARKER.to_string());
                    stripped = true;
                } else {
                    stripped |= strip_key_deep(v, key);
                }
            }
            stripped
        }
        Value::Array(arr) => {
            let mut stripped = false;
            for v in arr.iter_mut() {
                stripped |= strip_key_deep(v, key);
            }
            stripped
        }
        _ => false,
    }
}

/// Entity-aware deep compaction with depth limit. Used by the
/// evidence/working-memory pipeline so that long tool outputs don't bloat the
/// persisted snapshot.
pub fn compact_value(value: &Value) -> Value {
    compact_value_depth(value, 0)
}

fn compact_value_depth(value: &Value, depth: usize) -> Value {
    if depth >= 3 {
        return match value {
            Value::String(s) => Value::String(truncate(s, 320)),
            other => other.clone(),
        };
    }
    match value {
        Value::Object(map) => {
            let preferred = [
                "id",
                "entity_id",
                "note_id",
                "type",
                "entity_type",
                "title",
                "author",
                "url",
                "resolved_url",
                "summary",
                "content_summary",
                "key_points",
                "top_comments",
                "likes",
                "comments_count",
                "favorites",
                "screenshot",
                "artifact_path",
            ];
            let mut ordered: Vec<String> = preferred
                .iter()
                .filter_map(|p| {
                    if map.contains_key(*p) {
                        Some((*p).to_string())
                    } else {
                        None
                    }
                })
                .collect();
            for key in map.keys() {
                if !ordered.iter().any(|k| k == key) {
                    ordered.push(key.clone());
                }
            }
            let mut out = Map::new();
            for key in ordered.iter().take(20) {
                if let Some(v) = map.get(key) {
                    out.insert(key.clone(), compact_value_depth(v, depth + 1));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(
            arr.iter()
                .take(8)
                .map(|v| compact_value_depth(v, depth + 1))
                .collect(),
        ),
        Value::String(s) => Value::String(truncate(s, 600)),
        other => other.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn truncate_short_passthrough() {
        assert_eq!(truncate("hello", 32), "hello");
    }

    #[test]
    fn truncate_long_suffix() {
        let out = truncate(&"a".repeat(40), 10);
        assert!(out.starts_with("aaaaaaaaaa"));
        assert!(out.ends_with("[truncated]"));
        assert!(out.chars().count() > 10);
    }

    #[test]
    fn compact_json_picks_preferred_keys_first() {
        let value = json!({
            "z_extra": "x",
            "ok": true,
            "summary": "hi"
        });
        let compact = compact_json_value(&value);
        let keys: Vec<&str> = compact
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.as_str())
            .collect();
        assert_eq!(keys[0], "ok");
        assert_eq!(keys[1], "summary");
    }

    #[test]
    fn compress_text_falls_through_when_short() {
        let s = "small payload";
        assert_eq!(compress_text_maybe_json(s, 100), s);

        let notes: Vec<Value> = (0..12)
            .map(|index| {
                json!({
                    "entity": {
                        "note_id": format!("note-{index}"),
                        "url": format!("https://www.xiaohongshu.com/explore/note-{index}"),
                        "title": format!("title {index}"),
                        "author": format!("author {index}"),
                        "author_id": format!("author-{index}"),
                        "date": "2026-08-21",
                        "type": "normal",
                        "likes": "1.2万",
                        "favorites": "880",
                        "comments_count": "61",
                        "content": "body ".repeat(300),
                        "top_comments": [{
                            "text": "这个型号值得买吗？".repeat(30),
                            "author": "reader",
                            "sub_comments": [{"text": "长期使用后值得", "author": "author", "is_author": true}]
                        }],
                        "ocr_text": ["ocr".repeat(2000)]
                    }
                })
            })
            .collect();
        let oversized = json!({
            "query": "扫地机器人",
            "notes": notes,
            "artifact": {"path": "artifacts/search/robot.json"}
        })
        .to_string();
        let compacted = compress_text_maybe_json(&oversized, 30_000);
        let value: Value = serde_json::from_str(&compacted).unwrap();
        let compacted_notes = value["notes"].as_array().unwrap();

        assert_eq!(compacted_notes.len(), 12);
        assert_eq!(
            compacted_notes[11]["entity"]["author_id"],
            json!("author-11")
        );
        assert_eq!(compacted_notes[11]["entity"]["comments_count"], json!("61"));
        assert_eq!(
            compacted_notes[0]["entity"]["top_comments"][0]["sub_comments"][0]["is_author"],
            json!(true)
        );
        assert!(compacted.chars().count() <= 30_000);

        let dense_notes: Vec<Value> = (0..60)
            .map(|index| {
                json!({
                    "entity": {
                        "note_id": format!("dense-note-{index}"),
                        "title": "title".repeat(500),
                        "author": "author".repeat(500),
                        "url": format!("https://example.test/{}/{}", index, "path".repeat(500)),
                        "content": "body".repeat(1000)
                    }
                })
            })
            .collect();
        let dense = json!({
            "notes": dense_notes,
            "artifact": {"path": "artifacts/search/dense.json"}
        })
        .to_string();
        let dense_compacted = compress_text_maybe_json(&dense, 10_000);
        let dense_value: Value = serde_json::from_str(&dense_compacted).unwrap();
        assert_eq!(dense_value["notes"].as_array().unwrap().len(), 60);
        assert_eq!(
            dense_value["notes"][59]["entity"]["note_id"],
            "dense-note-59"
        );
        assert!(dense_compacted.chars().count() <= 10_000);

        let bounded = truncate_result(&"x".repeat(100), 20);
        assert_eq!(bounded.chars().count(), 20);
    }
}
