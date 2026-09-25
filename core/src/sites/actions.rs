//! Durable, process-safe receipts for social-network write operations.
//!
//! Platform adapters discover controls and perform trusted browser input. This
//! module reserves exactly one submit attempt before the click. A crash or
//! disconnect after reservation is deliberately ambiguous and must be resolved
//! by read-only reconciliation; it can never cause an automatic retry.

#[cfg(not(windows))]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const ACTION_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocialActionKind {
    Publish,
    Comment,
    Reply,
    Like,
    Follow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocialActionStatus {
    Draft,
    Prepared,
    Committing,
    Committed,
    CommitUnknown,
    Reconciled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionTarget {
    pub id: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionActor {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ActionPreview {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub evidence: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionReceipt {
    schema_version: u32,
    action_id: String,
    platform: String,
    action: SocialActionKind,
    target: ActionTarget,
    actor: ActionActor,
    preview: ActionPreview,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    status: SocialActionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    commit_started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    precommit_target_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    committed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reconciled_committed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reconciled_target_id: Option<String>,
}

impl ActionReceipt {
    pub fn action_id(&self) -> &str {
        &self.action_id
    }

    pub fn platform(&self) -> &str {
        &self.platform
    }

    pub fn action(&self) -> SocialActionKind {
        self.action
    }

    pub fn target(&self) -> &ActionTarget {
        &self.target
    }

    pub fn actor(&self) -> &ActionActor {
        &self.actor
    }

    pub fn preview(&self) -> &ActionPreview {
        &self.preview
    }

    pub fn status(&self) -> SocialActionStatus {
        self.status
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    pub fn commit_started_at(&self) -> Option<DateTime<Utc>> {
        self.commit_started_at
    }

    pub fn precommit_target_ids(&self) -> Option<&[String]> {
        self.precommit_target_ids.as_deref()
    }

    pub fn reconciled_committed(&self) -> Option<bool> {
        self.reconciled_committed
    }

    fn same_intent(
        &self,
        platform: &str,
        action: SocialActionKind,
        target: &ActionTarget,
        actor: &ActionActor,
        preview: &ActionPreview,
    ) -> bool {
        self.platform == platform
            && self.action == action
            && self.target == *target
            && self.actor == *actor
            && self.preview == *preview
    }

    fn require_status(&self, expected: SocialActionStatus) -> anyhow::Result<()> {
        if self.status != expected {
            anyhow::bail!(
                "invalid action transition: expected {expected:?}, found {:?}",
                self.status
            );
        }
        Ok(())
    }

    fn validate(&self) -> anyhow::Result<()> {
        if self.schema_version != ACTION_SCHEMA_VERSION {
            anyhow::bail!("unsupported action receipt schema");
        }
        validate_action_id(&self.action_id)?;
        validate_intent(
            &self.platform,
            self.action,
            &self.target,
            &self.actor,
            &self.preview,
        )?;
        if self.expires_at < self.created_at {
            anyhow::bail!("action receipt expires before it was created");
        }
        match self.status {
            SocialActionStatus::Draft | SocialActionStatus::Prepared => {
                if self.commit_started_at.is_some()
                    || self.precommit_target_ids.is_some()
                    || self.committed_at.is_some()
                    || self.reconciled_committed.is_some()
                    || self.reconciled_target_id.is_some()
                {
                    anyhow::bail!("uncommitted action contains terminal evidence");
                }
            }
            SocialActionStatus::Committing => {
                if self.commit_started_at.is_none()
                    || self.precommit_target_ids.is_none()
                    || self.committed_at.is_some()
                    || self.reconciled_committed.is_some()
                    || self.reconciled_target_id.is_some()
                {
                    anyhow::bail!("committing action contains inconsistent evidence");
                }
            }
            SocialActionStatus::Committed => {
                if self.commit_started_at.is_none()
                    || self.precommit_target_ids.is_none()
                    || self.committed_at.is_none()
                    || self.reconciled_committed.is_some()
                    || self.reconciled_target_id.is_some()
                {
                    anyhow::bail!("committed action contains inconsistent evidence");
                }
            }
            SocialActionStatus::CommitUnknown => {
                if self.commit_started_at.is_none()
                    || self.precommit_target_ids.is_none()
                    || self.committed_at.is_some()
                    || self.reconciled_committed.is_some()
                    || self.reconciled_target_id.is_some()
                {
                    anyhow::bail!("commit_unknown action contains inconsistent evidence");
                }
            }
            SocialActionStatus::Reconciled => {
                if self.commit_started_at.is_none()
                    || self.precommit_target_ids.is_none()
                    || self.reconciled_committed != Some(true)
                    || self.committed_at.is_none()
                    || self
                        .reconciled_target_id
                        .as_deref()
                        .unwrap_or_default()
                        .is_empty()
                {
                    anyhow::bail!("reconciled action must contain positive commit evidence");
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ActionStore {
    root: PathBuf,
}

impl ActionStore {
    pub fn open_default() -> Self {
        Self::new(crate::agent::file_bash_tools::socai_home_dir().join("actions"))
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn evidence_dir(&self, action_id: &str) -> anyhow::Result<PathBuf> {
        validate_action_id(action_id)?;
        let directory = self.root.join("evidence").join(action_id);
        ensure_private_directory(&directory)?;
        Ok(directory)
    }

    pub fn action_id_for(idempotency_key: &str) -> anyhow::Result<String> {
        validate_non_empty("idempotency_key", idempotency_key)?;
        Ok(stable_action_id(idempotency_key.trim()))
    }

    pub fn create_draft(
        &self,
        idempotency_key: &str,
        platform: impl Into<String>,
        action: SocialActionKind,
        target: ActionTarget,
        actor: ActionActor,
        preview: ActionPreview,
    ) -> anyhow::Result<ActionReceipt> {
        let platform = platform.into().trim().to_ascii_lowercase();
        validate_non_empty("idempotency_key", idempotency_key)?;
        validate_intent(&platform, action, &target, &actor, &preview)?;
        let action_id = stable_action_id(idempotency_key.trim());
        self.with_lock(|| {
            if self.receipt_path(&action_id).exists() {
                let receipt = self.load_locked(&action_id)?;
                if !receipt.same_intent(&platform, action, &target, &actor, &preview) {
                    anyhow::bail!("idempotency key is bound to a different action intent");
                }
                return Ok(receipt);
            }
            let now = Utc::now();
            let receipt = ActionReceipt {
                schema_version: ACTION_SCHEMA_VERSION,
                action_id,
                platform,
                action,
                target,
                actor,
                preview,
                created_at: now,
                expires_at: now,
                status: SocialActionStatus::Draft,
                commit_started_at: None,
                precommit_target_ids: None,
                committed_at: None,
                reconciled_committed: None,
                reconciled_target_id: None,
            };
            self.save_locked(&receipt)?;
            Ok(receipt)
        })
    }

    pub fn mark_prepared(
        &self,
        action_id: &str,
        observed_actor_id: &str,
        observed_target_id: &str,
        ttl_seconds: i64,
    ) -> anyhow::Result<ActionReceipt> {
        self.transition(action_id, |receipt| {
            receipt.require_status(SocialActionStatus::Draft)?;
            verify_identity(receipt, observed_actor_id, observed_target_id)?;
            receipt.expires_at = Utc::now() + Duration::seconds(ttl_seconds.clamp(1, 600));
            receipt.status = SocialActionStatus::Prepared;
            Ok(())
        })
    }

    /// Refresh immutable evidence while an action is still an unsubmitted
    /// draft. This supports rebuilding a creator draft after a harmless upload
    /// interruption without changing an already prepared/committing action.
    pub fn replace_draft_preview(
        &self,
        action_id: &str,
        observed_actor_id: &str,
        observed_target_id: &str,
        preview: ActionPreview,
    ) -> anyhow::Result<ActionReceipt> {
        self.transition(action_id, |receipt| {
            receipt.require_status(SocialActionStatus::Draft)?;
            verify_identity(receipt, observed_actor_id, observed_target_id)?;
            validate_intent(
                &receipt.platform,
                receipt.action,
                &receipt.target,
                &receipt.actor,
                &preview,
            )?;
            receipt.preview = preview;
            Ok(())
        })
    }

    /// A prepared action has not submitted anything, so it may safely return
    /// to Draft when its browser editor disappeared or its TTL elapsed.
    pub fn reset_prepared(
        &self,
        action_id: &str,
        observed_actor_id: &str,
        observed_target_id: &str,
    ) -> anyhow::Result<ActionReceipt> {
        self.transition(action_id, |receipt| {
            receipt.require_status(SocialActionStatus::Prepared)?;
            verify_identity(receipt, observed_actor_id, observed_target_id)?;
            receipt.status = SocialActionStatus::Draft;
            receipt.expires_at = receipt.created_at;
            Ok(())
        })
    }

    pub fn begin_commit(
        &self,
        action_id: &str,
        observed_actor_id: &str,
        observed_target_id: &str,
        precommit_target_ids: Vec<String>,
    ) -> anyhow::Result<ActionReceipt> {
        if precommit_target_ids
            .iter()
            .any(|value| value.trim().is_empty())
        {
            anyhow::bail!("precommit target IDs must not contain empty values");
        }
        self.transition(action_id, |receipt| {
            receipt.require_status(SocialActionStatus::Prepared)?;
            if Utc::now() > receipt.expires_at {
                anyhow::bail!("prepared action expired; run prepare-publish again");
            }
            verify_identity(receipt, observed_actor_id, observed_target_id)?;
            receipt.status = SocialActionStatus::Committing;
            receipt.commit_started_at = Some(Utc::now());
            receipt.precommit_target_ids = Some(precommit_target_ids);
            Ok(())
        })
    }

    pub fn finish_commit(&self, action_id: &str, verified: bool) -> anyhow::Result<ActionReceipt> {
        self.transition(action_id, |receipt| {
            receipt.require_status(SocialActionStatus::Committing)?;
            if verified {
                receipt.status = SocialActionStatus::Committed;
                receipt.committed_at = Some(Utc::now());
            } else {
                receipt.status = SocialActionStatus::CommitUnknown;
            }
            Ok(())
        })
    }

    pub fn reconcile_committed(
        &self,
        action_id: &str,
        reconciled_target_id: &str,
    ) -> anyhow::Result<ActionReceipt> {
        validate_non_empty("reconciled_target_id", reconciled_target_id)?;
        self.transition(action_id, |receipt| {
            if !matches!(
                receipt.status,
                SocialActionStatus::Committing
                    | SocialActionStatus::Committed
                    | SocialActionStatus::CommitUnknown
            ) {
                anyhow::bail!("action is not eligible for reconciliation");
            }
            if receipt
                .precommit_target_ids
                .as_ref()
                .is_some_and(|ids| ids.iter().any(|id| id == reconciled_target_id.trim()))
            {
                anyhow::bail!("reconciliation target existed before the submit reservation");
            }
            receipt.status = SocialActionStatus::Reconciled;
            receipt.reconciled_committed = Some(true);
            receipt.reconciled_target_id = Some(reconciled_target_id.trim().to_string());
            if receipt.committed_at.is_none() {
                receipt.committed_at = Some(Utc::now());
            }
            Ok(())
        })
    }

    pub fn load(&self, action_id: &str) -> anyhow::Result<ActionReceipt> {
        validate_action_id(action_id)?;
        self.with_lock(|| self.load_locked(action_id))
    }

    pub fn load_optional(&self, action_id: &str) -> anyhow::Result<Option<ActionReceipt>> {
        validate_action_id(action_id)?;
        self.with_lock(|| {
            if self.receipt_path(action_id).exists() {
                self.load_locked(action_id).map(Some)
            } else {
                Ok(None)
            }
        })
    }

    fn transition(
        &self,
        action_id: &str,
        update: impl FnOnce(&mut ActionReceipt) -> anyhow::Result<()>,
    ) -> anyhow::Result<ActionReceipt> {
        validate_action_id(action_id)?;
        self.with_lock(|| {
            let mut receipt = self.load_locked(action_id)?;
            update(&mut receipt)?;
            self.save_locked(&receipt)?;
            Ok(receipt)
        })
    }

    fn with_lock<T>(&self, operation: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<T> {
        ensure_private_directory(&self.root)?;
        ensure_private_directory(&self.receipts_dir())?;
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(self.root.join("actions.lock"))?;
        set_private_file_permissions(&self.root.join("actions.lock"))?;
        lock.lock_exclusive()?;
        let result = operation();
        let unlock = FileExt::unlock(&lock);
        match (result, unlock) {
            (Ok(value), Ok(())) => Ok(value),
            (Ok(_), Err(error)) => Err(error.into()),
            (Err(error), _) => Err(error),
        }
    }

    fn receipts_dir(&self) -> PathBuf {
        self.root.join("receipts")
    }

    fn receipt_path(&self, action_id: &str) -> PathBuf {
        self.receipts_dir().join(format!("{action_id}.json"))
    }

    fn load_locked(&self, action_id: &str) -> anyhow::Result<ActionReceipt> {
        let receipt: ActionReceipt =
            serde_json::from_slice(&fs::read(self.receipt_path(action_id))?)?;
        receipt.validate()?;
        if receipt.action_id != action_id {
            anyhow::bail!("receipt action_id does not match its filename");
        }
        Ok(receipt)
    }

    fn save_locked(&self, receipt: &ActionReceipt) -> anyhow::Result<()> {
        receipt.validate()?;
        let destination = self.receipt_path(&receipt.action_id);
        let temporary =
            self.receipts_dir()
                .join(format!(".{}.{}.tmp", receipt.action_id, Uuid::new_v4()));
        let result = (|| -> anyhow::Result<()> {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            set_private_file_permissions(&temporary)?;
            file.write_all(&serde_json::to_vec_pretty(receipt)?)?;
            file.sync_all()?;
            replace_file(&temporary, &destination)?;
            sync_directory(&self.receipts_dir())?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn stable_action_id(idempotency_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"socai-social-action-v1\0");
    hasher.update(idempotency_key.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn verify_identity(
    receipt: &ActionReceipt,
    observed_actor_id: &str,
    observed_target_id: &str,
) -> anyhow::Result<()> {
    if receipt.actor.id != observed_actor_id {
        anyhow::bail!("actor changed after prepare; refusing to submit");
    }
    if receipt.target.id != observed_target_id {
        anyhow::bail!("publish target changed after prepare; refusing to submit");
    }
    Ok(())
}

fn validate_intent(
    platform: &str,
    action: SocialActionKind,
    target: &ActionTarget,
    actor: &ActionActor,
    preview: &ActionPreview,
) -> anyhow::Result<()> {
    validate_non_empty("platform", platform)?;
    validate_non_empty("target.id", &target.id)?;
    validate_non_empty("target.url", &target.url)?;
    validate_non_empty("actor.id", &actor.id)?;
    let url = reqwest::Url::parse(target.url.trim())?;
    if url.scheme() != "https" || url.host_str().is_none() {
        anyhow::bail!("target.url must be an absolute HTTPS URL");
    }
    if matches!(
        action,
        SocialActionKind::Publish | SocialActionKind::Comment | SocialActionKind::Reply
    ) {
        validate_non_empty("preview.text", preview.text.as_deref().unwrap_or_default())?;
    }
    Ok(())
}

fn validate_action_id(action_id: &str) -> anyhow::Result<()> {
    if action_id.len() != 64
        || !action_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        anyhow::bail!("invalid action_id");
    }
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("{field} must not be empty");
    }
    Ok(())
}

fn ensure_private_directory(path: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(path)?;
    set_private_directory_permissions(path)
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> anyhow::Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(temporary, destination)
}

#[cfg(windows)]
fn replace_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source = temporary
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    let target = destination
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
