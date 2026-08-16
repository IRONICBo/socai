//! XHS analysis cache with conversation-scoped de-duplication. Note entities
//! and enrichment coverage are stored once in a global asset map; each
//! conversation stores only the note ids it has seen.
//!
//! - In-run dedup still lives on `ToolContext::processed_notes`; this store
//!   carries the same decision across follow-up runs in one conversation.
//! - A conversation reference gates de-duplication. A note seen only in a
//!   different conversation remains eligible, while conversations that already
//!   reference it reuse the latest global asset.
//! - Besides the lookup metadata (`note_id`, `title`, `author`, `url`, `level`,
//!   `include_media`, `analysis_count`, `first_seen_at`, `last_seen_at`) we also
//!   cache the full last-read `entity` (body + comments + images + location), so
//!   a reused note returns its complete data instead of degrading to the bare
//!   search card. Run dirs and artifact paths are dropped — they live in the
//!   per-run logs and would mostly point at stale paths anyway.
//! - File at `~/.socai/xhs/history.json` (overridable via `SOCAI_HOME`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub note_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub url: String,
    /// Deepest level ever recorded: "card" | "lite" | "deep".
    #[serde(default)]
    pub level: String,
    /// True once any past read had media (vision) enabled.
    #[serde(default)]
    pub include_media: bool,
    /// The cached entity holds downloaded media (an image or the video carries
    /// a `local_path`). Derived from the entity on every record, so it always
    /// states what the cache can actually serve.
    #[serde(default)]
    pub downloaded: bool,
    /// The cached entity holds OCR output. Derived like `downloaded`.
    #[serde(default)]
    pub ocr: bool,
    /// The cached entity holds a non-empty video audio transcript. Derived
    /// like `downloaded`.
    #[serde(default)]
    pub transcribed: bool,
    /// Most comments (primary + replies) ever cached for this note. Lets a later
    /// scan asking for more comments than we have re-read instead of short-
    /// circuiting on the stale, smaller set.
    #[serde(default)]
    pub comments_loaded: u32,
    /// The note's own total comment count (from `comments_count`, includes
    /// replies) as last seen. When `comments_loaded` reaches this we've captured
    /// everything, so a bigger request is still satisfied. `0` when unknown.
    #[serde(default)]
    pub comments_total: u32,
    #[serde(default)]
    pub analysis_count: u32,
    #[serde(default)]
    pub first_seen_at: String,
    #[serde(default)]
    pub last_seen_at: String,
    /// Full last-read entity (body, comments, images, location, …) so a reused
    /// note can be returned complete without re-opening it. `None` for entries
    /// written before this field existed or recorded from a card only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct HistoryFile {
    /// Schema version 0/1 used the global `notes` map alone; version 3 copied
    /// complete entries into `session_notes`; version 4 keeps global assets and
    /// conversation references separately.
    #[serde(default)]
    version: u32,
    /// Canonical note assets. Existing version 0/1 files already use this
    /// shape, so their entities stay readable without a destructive migration.
    #[serde(default)]
    notes: BTreeMap<String, HistoryEntry>,
    /// Lightweight conversation pointers. Presence of a note id in this set
    /// controls de-duplication; the corresponding entity lives in `notes`.
    #[serde(default)]
    session_refs: BTreeMap<String, BTreeSet<String>>,
    /// Version 3 compatibility input. `normalize_loaded_entries` moves these
    /// entries into `notes` + `session_refs`; the field is never serialized.
    #[serde(default, rename = "session_notes", skip_serializing)]
    legacy_session_notes: BTreeMap<String, BTreeMap<String, HistoryEntry>>,
    /// Deleted conversation ids. A tombstone prevents a background download
    /// that completed just after deletion from recreating that session.
    #[serde(default)]
    removed_sessions: BTreeSet<String>,
}

const HISTORY_VERSION: u32 = 4;

/// All store instances in this process share the same history file. Serialize
/// refresh/mutate/save sequences so foreground scans and background media
/// completions cannot overwrite each other's newer snapshots.
fn history_io_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub struct XhsHistoryStore {
    path: PathBuf,
    inner: Mutex<HistoryFile>,
}

impl XhsHistoryStore {
    /// `$SOCAI_HOME/xhs/history.json`, else `~/.socai/xhs/history.json`,
    /// else `.socai/xhs/history.json` relative to cwd.
    pub fn default_path() -> PathBuf {
        if let Ok(env) = std::env::var("SOCAI_HOME") {
            return PathBuf::from(env).join("xhs/history.json");
        }
        if let Some(home) = dirs::home_dir() {
            return home.join(".socai/xhs/history.json");
        }
        PathBuf::from(".socai/xhs/history.json")
    }

    pub fn open_default() -> Self {
        Self::open(Self::default_path())
    }

    pub fn open(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let _io = history_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut inner = load_file(&path).unwrap_or_default();
        normalize_loaded_entries(&mut inner);
        Self {
            path,
            inner: Mutex::new(inner),
        }
    }

    pub fn get(&self, session_id: &str, note_id: &str) -> Option<HistoryEntry> {
        let session_id = session_id.trim();
        let id = note_id.trim();
        if session_id.is_empty() || id.is_empty() {
            return None;
        }
        let _io = history_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        refresh_from_disk(&self.path, &mut guard);
        if !guard
            .session_refs
            .get(session_id)
            .is_some_and(|refs| refs.contains(id))
        {
            return None;
        }
        guard.notes.get(id).cloned()
    }

    /// True when we have the full cached entity for a note (so a reuse can
    /// return complete data). False for pre-upgrade entries that only stored
    /// lookup metadata — those should be re-read to backfill the cache.
    /// Cheap: checks presence without cloning the entity.
    pub fn has_cached_entity(&self, session_id: &str, note_id: &str) -> bool {
        let session_id = session_id.trim();
        let id = note_id.trim();
        if session_id.is_empty() || id.is_empty() {
            return false;
        }
        let _io = history_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        refresh_from_disk(&self.path, &mut guard);
        guard
            .session_refs
            .get(session_id)
            .is_some_and(|refs| refs.contains(id))
            && guard
                .notes
                .get(id)
                .is_some_and(|entry| entry.entity.is_some())
    }

    /// True when this session's prior analysis already covers what's being
    /// requested: this conversation references the note, recorded level is >=
    /// requested, and every requested enrichment (vision, downloaded media,
    /// OCR) is available in the shared asset. Another conversation can enrich
    /// that asset, but cannot create this conversation's reference or cause a
    /// first-time note to be skipped here.
    ///
    /// A download request is additionally checked against the disk: cached
    /// `local_path`s point into the run dir that downloaded them, and run dirs
    /// are user-deletable (deleting a task wipes them). Once any cached media
    /// file is gone the request is not satisfied, so the note is re-read and
    /// re-downloaded into the current run instead of being archived media-less
    /// on every future reuse.
    #[allow(clippy::too_many_arguments)]
    pub fn is_satisfied_by(
        &self,
        session_id: &str,
        note_id: &str,
        level: &str,
        include_media: bool,
        download_media: bool,
        ocr: bool,
        transcribe_audio: bool,
        min_comments: i64,
    ) -> bool {
        let Some(prev) = self.get(session_id, note_id) else {
            return false;
        };
        if level_value(&prev.level) < level_value(level) {
            return false;
        }
        if include_media && !prev.include_media {
            return false;
        }
        if download_media {
            if !prev.downloaded {
                return false;
            }
            if let Some(entity) = prev.entity.as_ref() {
                if !entity_media_intact(entity) {
                    return false;
                }
            }
        }
        if ocr && !prev.ocr {
            return false;
        }
        if transcribe_audio && !prev.transcribed {
            return false;
        }
        // A request for more comments than we cached forces a re-read — unless we
        // already captured the whole thread (loaded >= the note's own total), in
        // which case there is nothing more to fetch.
        if min_comments > 0
            && (prev.comments_loaded as i64) < min_comments
            && (prev.comments_total == 0 || prev.comments_loaded < prev.comments_total)
        {
            return false;
        }
        true
    }

    /// Add `already_analyzed` / `history_level` / `history_include_media`
    /// flags onto cards previously seen in this session. Mutates in place.
    pub fn annotate_cards(&self, session_id: &str, cards: &mut Value) {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return;
        }
        let _io = history_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        refresh_from_disk(&self.path, &mut guard);
        let Some(refs) = guard.session_refs.get(session_id) else {
            return;
        };
        annotate_cards_from_refs(&guard.notes, refs, cards);
    }

    /// Take an owned snapshot of all entries currently in the store. Use
    /// this when a tool mutates history during its own call (e.g.
    /// `search` records every note it reads) but still wants to
    /// annotate output cards based on what was known *before* the call —
    /// otherwise the annotation reflects this run's own writes.
    pub fn snapshot(&self, session_id: &str) -> HistorySnapshot {
        let session_id = session_id.trim();
        let _io = history_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        refresh_from_disk(&self.path, &mut guard);
        let entries = referenced_entries(&guard, session_id);
        HistorySnapshot { entries }
    }

    /// Add this note id to the conversation's references, then upsert its
    /// global asset. The cache is a growing union of information per note,
    /// kept consistent by construction:
    ///
    /// - `level` / `include_media` record the deepest *request* ever served
    ///   and never downgrade.
    /// - The cached `entity` merges the fresh read into what was already
    ///   cached ([`merge_entities`]): fresh fields win, enrichments only a
    ///   prior read produced (transcript, media paths, OCR text, a larger
    ///   comment set) are carried over rather than lost.
    /// - `downloaded` / `ocr` / `transcribed` are then *derived* from the
    ///   merged entity, so each flag states exactly whether the cache holds
    ///   that information. `is_satisfied_by` serves any request the flags
    ///   cover and forces a re-read for anything more — including retries
    ///   after failed attempts, since failures leave no information behind.
    /// - Attempt outcomes (`*_error` fields) are never cached: an error says
    ///   nothing durable about the note, and the absent information already
    ///   makes the next request that needs it re-read.
    pub fn record(&self, session_id: &str, entity: &Value, level: &str, include_media: bool) {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return;
        }
        let Some(note_id) = entity
            .get("note_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
        else {
            return;
        };
        let now = Utc::now().to_rfc3339();
        let title = string_field(entity, "title");
        let author = string_field(entity, "author");
        let url = string_field(entity, "url");
        let mut fresh = entity.clone();
        strip_error_fields(&mut fresh);

        let _io = history_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        refresh_from_disk(&self.path, &mut guard);
        guard.version = HISTORY_VERSION;
        if guard.removed_sessions.contains(session_id) {
            return;
        }
        guard
            .session_refs
            .entry(session_id.to_string())
            .or_default()
            .insert(note_id.clone());
        let entry = guard.notes.entry(note_id.clone()).or_insert_with(|| {
            let mut e = HistoryEntry::default();
            e.note_id = note_id.clone();
            e.first_seen_at = now.clone();
            e
        });
        entry.note_id = note_id;
        if !title.is_empty() {
            entry.title = title;
        }
        if !author.is_empty() {
            entry.author = author;
        }
        if !url.is_empty() {
            entry.url = url;
        }
        if level_value(level) > level_value(&entry.level) {
            entry.level = level.to_string();
        }
        if include_media {
            entry.include_media = true;
        }
        let merged = match entry.entity.take() {
            Some(prev) => merge_entities(&prev, fresh),
            None => fresh,
        };
        entry.downloaded = entity_has_downloaded(&merged);
        entry.ocr = entity_has_ocr(&merged);
        entry.transcribed = entity_has_transcript(&merged);
        entry.comments_loaded = entity_comment_count(&merged);
        let total = entity_comment_total(&merged);
        if total > entry.comments_total {
            entry.comments_total = total;
        }
        entry.entity = Some(merged);
        entry.analysis_count = entry.analysis_count.saturating_add(1);
        entry.last_seen_at = now;

        // Best-effort write. A failure here just means the next process
        // won't see this entry — agent still works.
        let _ = save_file(&self.path, &guard);
    }

    /// Forget downloaded media that lived under run dirs being deleted: strip
    /// the cached entities' `local_path`s pointing into them and downgrade the
    /// `downloaded` flag, so the next request re-reads (and re-downloads) the
    /// note instead of trusting paths that no longer exist. Returns how many
    /// entries were scrubbed.
    pub fn scrub_media_under(&self, run_dirs: &[PathBuf]) -> usize {
        let _io = history_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        refresh_from_disk(&self.path, &mut guard);
        let scrubbed = scrub_history_entries(&mut guard.notes, run_dirs);
        if scrubbed == 0 {
            return 0;
        }
        guard.version = HISTORY_VERSION;
        let _ = save_file(&self.path, &guard);
        scrubbed
    }

    /// Drop all dedup state for a deleted conversation.
    pub fn remove_session(&self, session_id: &str) -> bool {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return false;
        }
        let _io = history_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        refresh_from_disk(&self.path, &mut guard);
        let removed = guard.session_refs.remove(session_id).is_some();
        let tombstoned = guard.removed_sessions.insert(session_id.to_string());
        if !removed && !tombstoned {
            return false;
        }
        guard.version = HISTORY_VERSION;
        let _ = save_file(&self.path, &guard);
        true
    }
}

/// Owned snapshot of the history at a point in time. Cheap to pass around
/// since it's a plain map.
pub struct HistorySnapshot {
    entries: BTreeMap<String, HistoryEntry>,
}

impl HistorySnapshot {
    pub fn annotate_cards(&self, cards: &mut Value) {
        annotate_cards_from(&self.entries, cards);
    }
}

fn annotate_cards_from(entries: &BTreeMap<String, HistoryEntry>, cards: &mut Value) {
    annotate_cards_with(cards, |note_id| entries.get(note_id));
}

fn annotate_cards_from_refs(
    notes: &BTreeMap<String, HistoryEntry>,
    refs: &BTreeSet<String>,
    cards: &mut Value,
) {
    annotate_cards_with(cards, |note_id| {
        refs.contains(note_id).then(|| notes.get(note_id)).flatten()
    });
}

fn annotate_cards_with<'a>(
    cards: &mut Value,
    mut lookup: impl FnMut(&str) -> Option<&'a HistoryEntry>,
) {
    let Some(arr) = cards.as_array_mut() else {
        return;
    };
    for card in arr {
        let Some(map) = card.as_object_mut() else {
            continue;
        };
        let note_id = map
            .get("note_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let Some(note_id) = note_id else { continue };
        if let Some(entry) = lookup(&note_id) {
            map.insert("already_analyzed".into(), json!(true));
            map.insert("history_level".into(), json!(entry.level));
            map.insert("history_include_media".into(), json!(entry.include_media));
        }
    }
}

fn referenced_entries(data: &HistoryFile, session_id: &str) -> BTreeMap<String, HistoryEntry> {
    data.session_refs
        .get(session_id)
        .into_iter()
        .flatten()
        .filter_map(|note_id| {
            data.notes
                .get(note_id)
                .cloned()
                .map(|entry| (note_id.clone(), entry))
        })
        .collect()
}

fn scrub_history_entries(
    entries: &mut BTreeMap<String, HistoryEntry>,
    run_dirs: &[PathBuf],
) -> usize {
    let mut scrubbed = 0usize;
    for entry in entries.values_mut() {
        let Some(entity) = entry.entity.as_mut() else {
            continue;
        };
        if !scrub_entity_media_under(entity, run_dirs) {
            continue;
        }
        entry.downloaded = entity_has_downloaded(entity);
        scrubbed += 1;
    }
    scrubbed
}

fn level_value(level: &str) -> i32 {
    match level.trim().to_ascii_lowercase().as_str() {
        "deep" => 3,
        "lite" => 2,
        "card" => 1,
        _ => 0,
    }
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Count the comments cached on an entity: primary comments plus their replies.
/// `top_comments` may be full objects (with `sub_comments`) or already-leaned
/// (a string, or `{text, replies}`), so cover both shapes.
fn entity_comment_count(entity: &Value) -> u32 {
    let Some(comments) = entity.get("top_comments").and_then(Value::as_array) else {
        return 0;
    };
    let mut count = 0u32;
    for comment in comments {
        count += 1;
        let replies = comment
            .get("sub_comments")
            .or_else(|| comment.get("replies"))
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        count += replies as u32;
    }
    count
}

/// Parse the note's own total comment count from `comments_count` (e.g. "170",
/// "1.2万", "3,456"). `0` when absent or unparseable.
fn entity_comment_total(entity: &Value) -> u32 {
    let raw = entity
        .get("comments_count")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if raw.is_empty() {
        return 0;
    }
    let cleaned: String = raw
        .chars()
        .filter(|c| !matches!(c, ',' | '+' | ' '))
        .collect();
    let digits: String = cleaned
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let Ok(base) = digits.parse::<f64>() else {
        return 0;
    };
    let mult = if cleaned.contains('万') || cleaned.contains('w') || cleaned.contains('W') {
        10_000.0
    } else if cleaned.contains('k') || cleaned.contains('K') {
        1_000.0
    } else {
        1.0
    };
    (base * mult).round() as u32
}

/// True when the entity carries OCR output: a non-empty note-level `ocr_text`
/// array, any image with a non-empty per-image `ocr_text`, or a video with a
/// non-empty `poster_ocr` (the video-note cover OCR).
fn entity_has_ocr(entity: &Value) -> bool {
    let note_level = entity
        .get("ocr_text")
        .and_then(Value::as_array)
        .is_some_and(|arr| {
            arr.iter()
                .any(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()))
        });
    if note_level {
        return true;
    }
    let poster = entity
        .get("video")
        .and_then(|video| video.get("poster_ocr"))
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty());
    if poster {
        return true;
    }
    entity
        .get("images")
        .and_then(Value::as_array)
        .is_some_and(|imgs| {
            imgs.iter().any(|im| {
                im.get("ocr_text")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.trim().is_empty())
            })
        })
}

/// True when the entity's media was downloaded to disk (an image or the video
/// carries a non-empty `local_path`).
fn entity_has_downloaded(entity: &Value) -> bool {
    let has_local = |v: &Value| {
        v.get("local_path")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
    };
    let image_local = entity
        .get("images")
        .and_then(Value::as_array)
        .is_some_and(|imgs| imgs.iter().any(has_local));
    image_local || entity.get("video").is_some_and(has_local)
}

/// The on-disk media files a cached entity claims: image `local_path`s plus
/// the video's `local_path` / `poster_local_path` — the same fields the media
/// manifest validates and the app renders. Empty fields are skipped (they
/// mean "never downloaded", not "downloaded then lost").
fn entity_media_paths(entity: &Value) -> Vec<&str> {
    fn trimmed_path(value: Option<&Value>) -> Option<&str> {
        value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
    }
    let mut paths = Vec::new();
    if let Some(images) = entity.get("images").and_then(Value::as_array) {
        for image in images {
            paths.extend(trimmed_path(image.get("local_path")));
        }
    }
    if let Some(video) = entity.get("video") {
        paths.extend(trimmed_path(video.get("local_path")));
        paths.extend(trimmed_path(video.get("poster_local_path")));
    }
    paths
}

/// True while every media file the entity claims still is a non-empty file on
/// disk — the same bar the media manifest uses to call an asset "downloaded".
/// Cached paths are absolute (they point into the run dir that downloaded
/// them); a path we can't verify fails the check.
fn entity_media_intact(entity: &Value) -> bool {
    entity_media_paths(entity).into_iter().all(|path| {
        let path = Path::new(path);
        path.is_absolute() && fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() > 0)
    })
}

/// Remove the entity's image/video `local_path`-style fields that point under
/// any of `run_dirs`. True when anything was removed.
fn scrub_entity_media_under(entity: &mut Value, run_dirs: &[PathBuf]) -> bool {
    fn scrub_field(owner: &mut Value, key: &str, run_dirs: &[PathBuf]) -> bool {
        let hit = owner.get(key).and_then(Value::as_str).is_some_and(|path| {
            let path = Path::new(path.trim());
            run_dirs.iter().any(|dir| path.starts_with(dir))
        });
        if hit {
            if let Some(map) = owner.as_object_mut() {
                map.remove(key);
            }
        }
        hit
    }
    let mut changed = false;
    if let Some(images) = entity.get_mut("images").and_then(Value::as_array_mut) {
        for image in images {
            changed |= scrub_field(image, "local_path", run_dirs);
        }
    }
    if let Some(video) = entity.get_mut("video") {
        changed |= scrub_field(video, "local_path", run_dirs);
        changed |= scrub_field(video, "poster_local_path", run_dirs);
    }
    changed
}

fn entity_has_transcript(entity: &Value) -> bool {
    entity
        .get("video")
        .and_then(|video| video.get("transcript"))
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
}

/// Remove attempt outcomes (`*_error` fields) from an entity before caching:
/// on the note itself, its `video` object, and each image. An error describes
/// one failed attempt, not the note — caching it would keep re-serving a
/// stale failure after the underlying cause is gone, while the *absence* of
/// the information (via the derived flags) is what correctly drives a retry.
fn strip_error_fields(entity: &mut Value) {
    fn strip(obj: &mut serde_json::Map<String, Value>) {
        obj.retain(|key, _| !key.ends_with("_error"));
    }
    let Some(obj) = entity.as_object_mut() else {
        return;
    };
    strip(obj);
    if let Some(video) = obj.get_mut("video").and_then(Value::as_object_mut) {
        strip(video);
    }
    if let Some(images) = obj.get_mut("images").and_then(Value::as_array_mut) {
        for image in images {
            if let Some(image) = image.as_object_mut() {
                strip(image);
            }
        }
    }
}

/// Merge a fresh read into the previously cached entity so a lesser re-read
/// never erases information a prior read paid for. Fresh fields win — they
/// are newer — with three carry-overs of prior enrichment work:
///
/// - `video`: merged key by key; keys the fresh read didn't produce
///   (transcript, `local_path`, `poster_ocr`, …) are kept from the cache.
/// - `images`: kept from the cache when the fresh read collected none.
/// - `top_comments`: kept from the cache when it holds more than the fresh
///   read loaded.
///
/// The capability flags are re-derived from the merged result, so any
/// information this merge does drop (e.g. fresh images replacing OCR'd ones)
/// is reflected in the flags and re-acquired by the next request needing it.
fn merge_entities(prev: &Value, mut fresh: Value) -> Value {
    let Some(fresh_obj) = fresh.as_object_mut() else {
        return prev.clone();
    };
    if let Some(prev_video) = prev.get("video").and_then(Value::as_object) {
        let fresh_video = fresh_obj
            .entry("video")
            .or_insert_with(|| Value::Object(Default::default()));
        if let Some(fresh_video) = fresh_video.as_object_mut() {
            for (key, value) in prev_video {
                fresh_video
                    .entry(key.clone())
                    .or_insert_with(|| value.clone());
            }
        }
    }
    let fresh_has_images = fresh_obj
        .get("images")
        .and_then(Value::as_array)
        .is_some_and(|images| !images.is_empty());
    if !fresh_has_images {
        if let Some(prev_images) = prev
            .get("images")
            .and_then(Value::as_array)
            .filter(|images| !images.is_empty())
        {
            fresh_obj.insert("images".into(), Value::Array(prev_images.clone()));
            fresh_obj.insert("image_count".into(), json!(prev_images.len()));
        }
    }
    if entity_comment_count(prev) > entity_comment_count(&fresh) {
        if let Some(prev_comments) = prev.get("top_comments") {
            if let Some(fresh_obj) = fresh.as_object_mut() {
                fresh_obj.insert("top_comments".into(), prev_comments.clone());
            }
        }
    }
    fresh
}

/// Migration must retain fields from both serialized copies. The regular
/// runtime merge intentionally lets a fresh read replace ordinary fields, but
/// a schema conversion has no authoritative network read and must be lossless.
fn merge_migrated_entities(prev: &Value, fresh: Value) -> Value {
    let merged_images = match (
        prev.get("images").and_then(Value::as_array),
        fresh.get("images").and_then(Value::as_array),
    ) {
        (Some(prev), Some(fresh)) if !prev.is_empty() && !fresh.is_empty() => {
            Some(merge_migrated_images(prev, fresh))
        }
        _ => None,
    };
    let mut merged = merge_entities(prev, fresh);
    let (Some(prev), Some(merged_object)) = (prev.as_object(), merged.as_object_mut()) else {
        return merged;
    };
    for (key, value) in prev {
        merged_object
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
    if let Some(images) = merged_images {
        merged_object.insert("image_count".into(), json!(images.len()));
        merged_object.insert("images".into(), Value::Array(images));
    }
    merged
}

fn merge_migrated_images(prev: &[Value], fresh: &[Value]) -> Vec<Value> {
    let mut merged = fresh.to_vec();
    let fresh_len = fresh.len();
    for (prev_position, prev_image) in prev.iter().enumerate() {
        let prev_index = image_index(prev_image);
        let prev_url = image_url(prev_image);
        let match_position = prev_index
            .and_then(|index| {
                merged
                    .iter()
                    .position(|fresh_image| image_index(fresh_image) == Some(index))
            })
            .or_else(|| {
                prev_url.and_then(|url| {
                    merged
                        .iter()
                        .position(|fresh_image| image_url(fresh_image) == Some(url))
                })
            })
            .or_else(|| {
                (prev_index.is_none()
                    && prev_url.is_none()
                    && prev_position < fresh_len
                    && image_index(&merged[prev_position]).is_none()
                    && image_url(&merged[prev_position]).is_none())
                .then_some(prev_position)
            });
        if let Some(position) = match_position {
            merged[position] = merge_migrated_object(prev_image, &merged[position]);
        } else {
            merged.push(prev_image.clone());
        }
    }
    merged
}

fn merge_migrated_object(prev: &Value, fresh: &Value) -> Value {
    let (Some(prev), Some(fresh)) = (prev.as_object(), fresh.as_object()) else {
        return fresh.clone();
    };
    let mut merged = fresh.clone();
    for (key, prev_value) in prev {
        match merged.get_mut(key) {
            Some(fresh_value) if prev_value.is_object() && fresh_value.is_object() => {
                *fresh_value = merge_migrated_object(prev_value, fresh_value);
            }
            Some(_) => {}
            None => {
                merged.insert(key.clone(), prev_value.clone());
            }
        }
    }
    Value::Object(merged)
}

fn image_index(image: &Value) -> Option<i64> {
    image.get("index").and_then(Value::as_i64)
}

fn image_url(image: &Value) -> Option<&str> {
    image
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
}

fn normalize_history_entry(note_id: &str, entry: &mut HistoryEntry) {
    entry.note_id = note_id.to_string();
    let Some(entity) = entry.entity.as_mut() else {
        return;
    };
    strip_error_fields(entity);
    let entity = &*entity;
    entry.downloaded = entity_has_downloaded(entity);
    entry.ocr = entity_has_ocr(entity);
    entry.transcribed = entity_has_transcript(entity);
    entry.comments_loaded = entry.comments_loaded.max(entity_comment_count(entity));
    entry.comments_total = entry.comments_total.max(entity_comment_total(entity));
}

/// Fold a legacy or freshly loaded entry into the canonical global asset.
/// Timestamps select which entity supplies conflicting ordinary fields, while
/// all non-conflicting serialized fields are preserved from both copies.
fn merge_history_entry(
    notes: &mut BTreeMap<String, HistoryEntry>,
    note_id: &str,
    mut incoming: HistoryEntry,
) {
    normalize_history_entry(note_id, &mut incoming);
    let Some(existing) = notes.get_mut(note_id) else {
        notes.insert(note_id.to_string(), incoming);
        return;
    };

    let incoming_is_newer = incoming.last_seen_at.as_str() >= existing.last_seen_at.as_str();
    if existing.title.is_empty() || (incoming_is_newer && !incoming.title.is_empty()) {
        existing.title = incoming.title.clone();
    }
    if existing.author.is_empty() || (incoming_is_newer && !incoming.author.is_empty()) {
        existing.author = incoming.author.clone();
    }
    if existing.url.is_empty() || (incoming_is_newer && !incoming.url.is_empty()) {
        existing.url = incoming.url.clone();
    }
    if level_value(&incoming.level) > level_value(&existing.level) {
        existing.level = incoming.level.clone();
    }
    existing.include_media |= incoming.include_media;
    existing.downloaded |= incoming.downloaded;
    existing.ocr |= incoming.ocr;
    existing.transcribed |= incoming.transcribed;
    existing.comments_loaded = existing.comments_loaded.max(incoming.comments_loaded);
    existing.comments_total = existing.comments_total.max(incoming.comments_total);
    existing.analysis_count = existing
        .analysis_count
        .saturating_add(incoming.analysis_count);
    if existing.first_seen_at.is_empty()
        || (!incoming.first_seen_at.is_empty() && incoming.first_seen_at < existing.first_seen_at)
    {
        existing.first_seen_at = incoming.first_seen_at.clone();
    }
    if incoming.last_seen_at > existing.last_seen_at {
        existing.last_seen_at = incoming.last_seen_at.clone();
    }

    existing.entity = match (existing.entity.take(), incoming.entity.take()) {
        (Some(current), Some(incoming)) if incoming_is_newer => {
            Some(merge_migrated_entities(&current, incoming))
        }
        (Some(current), Some(incoming)) => Some(merge_migrated_entities(&incoming, current)),
        (Some(current), None) => Some(current),
        (None, Some(incoming)) => Some(incoming),
        (None, None) => None,
    };
    normalize_history_entry(note_id, existing);
}

/// Re-establish the cache invariants on entries written by older versions:
/// - v0/v1 global `notes` remain the canonical assets;
/// - v3 per-session entries are merged into those assets and replaced by id
///   references;
/// - cached entities drop attempt outcomes and their capability flags are
///   re-derived.
///
/// This is in-memory and persists with the next regular save.
fn normalize_loaded_entries(data: &mut HistoryFile) {
    data.version = HISTORY_VERSION;

    let mut notes = BTreeMap::new();
    for (map_id, entry) in std::mem::take(&mut data.notes) {
        let note_id = if map_id.trim().is_empty() {
            entry.note_id.trim().to_string()
        } else {
            map_id.trim().to_string()
        };
        if !note_id.is_empty() {
            merge_history_entry(&mut notes, &note_id, entry);
        }
    }
    data.notes = notes;

    for (session_id, entries) in std::mem::take(&mut data.legacy_session_notes) {
        let session_id = session_id.trim();
        if session_id.is_empty() || data.removed_sessions.contains(session_id) {
            continue;
        }
        for (map_id, entry) in entries {
            let note_id = if map_id.trim().is_empty() {
                entry.note_id.trim().to_string()
            } else {
                map_id.trim().to_string()
            };
            if note_id.is_empty() {
                continue;
            }
            merge_history_entry(&mut data.notes, &note_id, entry);
            data.session_refs
                .entry(session_id.to_string())
                .or_default()
                .insert(note_id);
        }
    }

    let asset_ids = data.notes.keys().cloned().collect::<BTreeSet<_>>();
    let mut session_refs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (session_id, refs) in std::mem::take(&mut data.session_refs) {
        let session_id = session_id.trim();
        if session_id.is_empty() || data.removed_sessions.contains(session_id) {
            continue;
        }
        let refs = refs
            .into_iter()
            .map(|note_id| note_id.trim().to_string())
            .filter(|note_id| !note_id.is_empty() && asset_ids.contains(note_id))
            .collect::<BTreeSet<_>>();
        if !refs.is_empty() {
            session_refs
                .entry(session_id.to_string())
                .or_default()
                .extend(refs);
        }
    }
    data.session_refs = session_refs;
}

fn refresh_from_disk(path: &Path, current: &mut HistoryFile) {
    if let Some(mut latest) = load_file(path) {
        normalize_loaded_entries(&mut latest);
        *current = latest;
    }
}

fn load_file(path: &Path) -> Option<HistoryFile> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn save_file(path: &Path, data: &HistoryFile) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

// AGENTS.md: do NOT add new Rust tests unless the user explicitly asks. Update
// the existing ones when an API they cover changes; don't grow this module.
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn records_and_recalls_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history.json");
        let store = XhsHistoryStore::open(&path);

        store.record(
            "session-a",
            &json!({"note_id": "abc", "title": "T", "author": "A", "url": "u"}),
            "lite",
            false,
        );
        let entry = store.get("session-a", "abc").expect("entry present");
        assert_eq!(entry.note_id, "abc");
        assert_eq!(entry.level, "lite");
        assert_eq!(entry.analysis_count, 1);
        assert!(!entry.first_seen_at.is_empty());
        assert!(store.get("session-b", "abc").is_none());
        let persisted: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(persisted["version"], json!(HISTORY_VERSION));
        assert!(persisted["notes"]["abc"].is_object());
        assert_eq!(persisted["session_refs"]["session-a"], json!(["abc"]));
        assert!(persisted.get("session_notes").is_none());

        // Reopen from disk — both the cache and session boundary persist.
        let store2 = XhsHistoryStore::open(&path);
        assert!(store2.get("session-a", "abc").is_some());
        assert!(store2.get("session-b", "abc").is_none());

        // Separate store instances (foreground/background tool factories)
        // merge against the latest file instead of overwriting one another.
        let writers = (0..4)
            .map(|index| {
                let path = path.clone();
                std::thread::spawn(move || {
                    XhsHistoryStore::open(path).record(
                        &format!("concurrent-{index}"),
                        &json!({"note_id": format!("note-{index}")}),
                        "lite",
                        false,
                    );
                })
            })
            .collect::<Vec<_>>();
        for writer in writers {
            writer.join().unwrap();
        }
        let merged = XhsHistoryStore::open(&path);
        for index in 0..4 {
            assert!(merged
                .get(&format!("concurrent-{index}"), &format!("note-{index}"))
                .is_some());
        }

        // A v1 global asset has no trustworthy conversation owner. It remains
        // readable and merges with new reads, but cannot suppress a v4 session
        // until that session records a reference itself.
        let legacy_path = dir.path().join("legacy.json");
        std::fs::write(
            &legacy_path,
            serde_json::to_vec(&json!({
                "notes": {
                    "legacy": {"note_id": "legacy", "level": "deep"}
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let legacy = XhsHistoryStore::open(&legacy_path);
        assert!(legacy.get("session-a", "legacy").is_none());
        legacy.record("session-a", &json!({"note_id": "legacy"}), "lite", false);
        assert_eq!(legacy.get("session-a", "legacy").unwrap().level, "deep");

        // A v3 file copied complete entries into every session. Opening it
        // folds those entries into global assets and keeps only id pointers;
        // the next normal write persists the v4 shape without data loss.
        let v3_path = dir.path().join("v3.json");
        std::fs::write(
            &v3_path,
            serde_json::to_vec(&json!({
                "version": 3,
                "notes": {
                    "from-v3": {
                        "note_id": "from-v3",
                        "level": "lite",
                        "entity": {
                            "note_id": "from-v3",
                            "global_field": "preserved global content",
                            "images": [
                                {
                                    "index": 0,
                                    "url": "https://img.example/0.jpg",
                                    "local_path": "/tmp/preserved-image.jpg"
                                },
                                {
                                    "index": 1,
                                    "url": "https://img.example/old.jpg",
                                    "local_path": "/tmp/old-image.jpg"
                                }
                            ]
                        }
                    }
                },
                "session_notes": {
                    "session-v3": {
                        "from-v3": {
                            "note_id": "from-v3",
                            "level": "deep",
                            "ocr": true,
                            "entity": {
                                "note_id": "from-v3",
                                "desc": "preserved v3 content",
                                "ocr_text": ["preserved OCR"],
                                "images": [
                                    {
                                        "index": 0,
                                        "url": "https://img.example/0.jpg",
                                        "ocr_text": "preserved image OCR"
                                    },
                                    {
                                        "index": 2,
                                        "url": "https://img.example/new.jpg",
                                        "ocr_text": "new image OCR"
                                    }
                                ]
                            }
                        }
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let v3 = XhsHistoryStore::open(&v3_path);
        let migrated = v3.get("session-v3", "from-v3").unwrap();
        assert_eq!(
            migrated.entity.unwrap()["desc"],
            json!("preserved v3 content")
        );
        assert_eq!(
            v3.get("session-v3", "from-v3").unwrap().entity.unwrap()["global_field"],
            json!("preserved global content")
        );
        let migrated_image = &v3.get("session-v3", "from-v3").unwrap().entity.unwrap()["images"][0];
        assert_eq!(
            migrated_image["local_path"],
            json!("/tmp/preserved-image.jpg")
        );
        assert_eq!(migrated_image["ocr_text"], json!("preserved image OCR"));
        let migrated_images = v3.get("session-v3", "from-v3").unwrap().entity.unwrap()["images"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(migrated_images.len(), 3);
        assert!(migrated_images[1].get("local_path").is_none());
        assert_eq!(
            migrated_images[2]["local_path"],
            json!("/tmp/old-image.jpg")
        );
        assert!(v3.get("another-session", "from-v3").is_none());
        v3.record(
            "session-v3",
            &json!({"note_id": "new-v4-asset"}),
            "lite",
            false,
        );
        let migrated_file: Value =
            serde_json::from_slice(&std::fs::read(&v3_path).unwrap()).unwrap();
        assert!(migrated_file.get("session_notes").is_none());
        assert_eq!(
            migrated_file["session_refs"]["session-v3"],
            json!(["from-v3", "new-v4-asset"])
        );
        assert_eq!(
            migrated_file["notes"]["from-v3"]["entity"]["desc"],
            json!("preserved v3 content")
        );
        assert_eq!(
            migrated_file["notes"]["from-v3"]["entity"]["global_field"],
            json!("preserved global content")
        );
        assert_eq!(
            migrated_file["notes"]["from-v3"]["entity"]["images"][0]["local_path"],
            json!("/tmp/preserved-image.jpg")
        );
        assert_eq!(
            migrated_file["notes"]["from-v3"]["entity"]["images"][0]["ocr_text"],
            json!("preserved image OCR")
        );
        // Reopening and normalizing the persisted v4 file is idempotent.
        let reopened = XhsHistoryStore::open(&v3_path);
        let reopened_image = &reopened
            .get("session-v3", "from-v3")
            .unwrap()
            .entity
            .unwrap()["images"][0];
        assert_eq!(
            reopened_image["local_path"],
            json!("/tmp/preserved-image.jpg")
        );
        assert_eq!(reopened_image["ocr_text"], json!("preserved image OCR"));
        let reopened_images = reopened
            .get("session-v3", "from-v3")
            .unwrap()
            .entity
            .unwrap()["images"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(reopened_images.len(), 3);
        assert!(reopened_images[1].get("local_path").is_none());
        assert_eq!(
            reopened_images[2]["local_path"],
            json!("/tmp/old-image.jpg")
        );

        assert!(store2.remove_session("session-a"));
        // A detached background completion arriving after deletion cannot
        // recreate the removed conversation's history.
        store2.record(
            "session-a",
            &json!({"note_id": "late-background-write"}),
            "deep",
            true,
        );
        assert!(XhsHistoryStore::open(&path)
            .get("session-a", "abc")
            .is_none());
        assert!(XhsHistoryStore::open(&path)
            .get("session-a", "late-background-write")
            .is_none());
        let after_delete: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(after_delete["notes"]["abc"].is_object());
        assert!(after_delete["session_refs"].get("session-a").is_none());
    }

    #[test]
    fn level_never_downgrades_but_media_upgrades() {
        let dir = tempdir().unwrap();
        let store = XhsHistoryStore::open(dir.path().join("h.json"));

        store.record("session", &json!({"note_id": "n1"}), "deep", true);
        store.record("session", &json!({"note_id": "n1"}), "lite", false);
        let entry = store.get("session", "n1").unwrap();
        assert_eq!(entry.level, "deep");
        assert!(entry.include_media);
        assert_eq!(entry.analysis_count, 2);
    }

    #[test]
    fn satisfied_when_prior_is_deeper_or_equal() {
        let dir = tempdir().unwrap();
        let store = XhsHistoryStore::open(dir.path().join("h.json"));
        store.record("session", &json!({"note_id": "n1"}), "lite", false);

        assert!(store.is_satisfied_by("session", "n1", "card", false, false, false, false, 0));
        assert!(store.is_satisfied_by("session", "n1", "lite", false, false, false, false, 0));
        assert!(!store.is_satisfied_by(
            "other-session",
            "n1",
            "lite",
            false,
            false,
            false,
            false,
            0
        ));
        assert!(!store.is_satisfied_by("session", "n1", "deep", false, false, false, false, 0));
        assert!(!store.is_satisfied_by("session", "n1", "lite", true, false, false, false, 0));
        assert!(!store.is_satisfied_by("session", "unknown", "card", false, false, false, false, 0));
        // download / ocr dimensions: a plain read doesn't satisfy them.
        assert!(!store.is_satisfied_by("session", "n1", "lite", false, true, false, false, 0));
        assert!(!store.is_satisfied_by("session", "n1", "lite", false, false, true, false, 0));
        assert!(!store.is_satisfied_by("session", "n1", "lite", false, false, false, true, 0));

        // The second conversation must establish its own pointer before it can
        // skip. Once referenced, both conversations reuse the richer shared
        // asset rather than storing duplicate entities.
        store.record(
            "other-session",
            &json!({"note_id": "n1", "ocr_text": ["other session"]}),
            "deep",
            true,
        );
        assert!(store.is_satisfied_by("other-session", "n1", "deep", true, false, true, false, 0));
        assert!(store.is_satisfied_by("session", "n1", "deep", false, false, false, false, 0));
        assert!(store.is_satisfied_by("session", "n1", "lite", true, false, false, false, 0));
        assert!(store.is_satisfied_by("session", "n1", "lite", false, false, true, false, 0));
        assert!(!store.is_satisfied_by(
            "third-session",
            "n1",
            "lite",
            false,
            false,
            false,
            false,
            0
        ));
    }

    #[test]
    fn satisfied_tracks_downloaded_and_ocr_from_entity() {
        let dir = tempdir().unwrap();
        let store = XhsHistoryStore::open(dir.path().join("h.json"));
        let media = dir.path().join("x.jpg");
        std::fs::write(&media, b"jpg").unwrap();
        store.record(
            "session",
            &json!({
                "note_id": "n2",
                "ocr_text": ["cover text", ""],
                "images": [{"url": "u", "local_path": media.to_string_lossy(), "ocr_text": "cover text"}],
            }),
            "deep",
            false,
        );

        assert!(store.is_satisfied_by("session", "n2", "deep", false, true, true, false, 0));
        let entry = store.get("session", "n2").unwrap();
        assert!(entry.downloaded);
        assert!(entry.ocr);

        // Deleting the downloaded file voids download satisfaction (the note
        // must be re-read, not resurrected with a dead path) — while text-only
        // coverage is untouched.
        std::fs::remove_file(&media).unwrap();
        assert!(!store.is_satisfied_by("session", "n2", "deep", false, true, true, false, 0));
        assert!(store.is_satisfied_by("session", "n2", "deep", false, false, true, false, 0));
    }

    #[test]
    fn snapshot_freezes_pre_call_state() {
        let dir = tempdir().unwrap();
        let store = XhsHistoryStore::open(dir.path().join("h.json"));
        store.record("session", &json!({"note_id": "old"}), "lite", false);

        let pre = store.snapshot("session");
        // Writes after the snapshot must not show up when annotating with it.
        store.record("session", &json!({"note_id": "new_this_run"}), "deep", true);

        let mut cards = json!([
            {"note_id": "old"},
            {"note_id": "new_this_run"},
        ]);
        pre.annotate_cards(&mut cards);
        let arr = cards.as_array().unwrap();
        assert_eq!(arr[0]["already_analyzed"], json!(true));
        assert!(arr[1].get("already_analyzed").is_none());
    }

    #[test]
    fn annotate_cards_marks_known_notes() {
        let dir = tempdir().unwrap();
        let store = XhsHistoryStore::open(dir.path().join("h.json"));
        store.record(
            "session",
            &json!({"note_id": "seen", "title": "x"}),
            "deep",
            true,
        );

        let mut cards = json!([
            {"note_id": "seen", "title": "x"},
            {"note_id": "fresh", "title": "y"},
        ]);
        store.annotate_cards("session", &mut cards);
        let arr = cards.as_array().unwrap();
        assert_eq!(arr[0]["already_analyzed"], json!(true));
        assert_eq!(arr[0]["history_level"], json!("deep"));
        assert_eq!(arr[0]["history_include_media"], json!(true));
        assert!(arr[1].get("already_analyzed").is_none());
    }
}
