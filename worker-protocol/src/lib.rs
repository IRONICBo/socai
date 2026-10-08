//! Transport-neutral control commands and ordered event DTOs for socai workers.
//!
//! This crate intentionally contains wire types only, so an orchestrator can
//! version and validate the protocol without linking the Agent runtime.

#![forbid(unsafe_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CONTROL_PROTOCOL_VERSION: u32 = 2;
pub const EVENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionBinding {
    pub user_id: String,
    pub session_id: String,
    pub kernel_id: String,
    pub assignment_epoch: u64,
}

impl SessionBinding {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_id("user_id", &self.user_id)?;
        validate_id("session_id", &self.session_id)?;
        validate_id("kernel_id", &self.kernel_id)?;
        if self.assignment_epoch == 0 {
            return Err(ProtocolError::InvalidField {
                field: "assignment_epoch",
                message: "must be greater than zero".into(),
            });
        }
        Ok(())
    }
}

fn validate_id(field: &'static str, value: &str) -> Result<(), ProtocolError> {
    if value.trim().is_empty() || value.len() > 160 {
        return Err(ProtocolError::InvalidField {
            field,
            message: "must contain between 1 and 160 bytes".into(),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerCommand {
    #[serde(rename = "session.bind")]
    SessionBind {
        protocol_version: u32,
        request_id: String,
        binding: SessionBinding,
    },
    #[serde(rename = "run.start")]
    RunStart {
        protocol_version: u32,
        request_id: String,
        binding: SessionBinding,
        run_id: String,
        prompt: String,
        #[serde(default)]
        provider: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        enabled_sites: Vec<String>,
        #[serde(default)]
        max_steps: Option<u32>,
        #[serde(default)]
        max_tokens: Option<u32>,
    },
    #[serde(rename = "run.cancel")]
    RunCancel {
        protocol_version: u32,
        request_id: String,
        binding: SessionBinding,
        run_id: String,
    },
    #[serde(rename = "run.resume")]
    RunResume {
        protocol_version: u32,
        request_id: String,
        binding: SessionBinding,
        run_id: String,
    },
    Shutdown {
        protocol_version: u32,
        request_id: String,
    },
}

impl WorkerCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        let (version, request_id) = match self {
            Self::SessionBind {
                protocol_version,
                request_id,
                binding,
            } => {
                binding.validate()?;
                (*protocol_version, request_id)
            }
            Self::RunStart {
                protocol_version,
                request_id,
                binding,
                run_id,
                prompt,
                enabled_sites,
                max_steps,
                max_tokens,
                ..
            } => {
                binding.validate()?;
                validate_id("run_id", run_id)?;
                if prompt.trim().is_empty() || prompt.len() > 100_000 {
                    return Err(ProtocolError::InvalidField {
                        field: "prompt",
                        message: "must contain between 1 and 100000 bytes".into(),
                    });
                }
                if enabled_sites.len() > 16 {
                    return Err(ProtocolError::InvalidField {
                        field: "enabled_sites",
                        message: "supports at most 16 sites".into(),
                    });
                }
                if max_steps.is_some_and(|value| value == 0 || value > 100) {
                    return Err(ProtocolError::InvalidField {
                        field: "max_steps",
                        message: "must be between 1 and 100".into(),
                    });
                }
                if max_tokens.is_some_and(|value| !(256..=128_000).contains(&value)) {
                    return Err(ProtocolError::InvalidField {
                        field: "max_tokens",
                        message: "must be between 256 and 128000".into(),
                    });
                }
                (*protocol_version, request_id)
            }
            Self::RunCancel {
                protocol_version,
                request_id,
                binding,
                run_id,
            }
            | Self::RunResume {
                protocol_version,
                request_id,
                binding,
                run_id,
            } => {
                binding.validate()?;
                validate_id("run_id", run_id)?;
                (*protocol_version, request_id)
            }
            Self::Shutdown {
                protocol_version,
                request_id,
            } => (*protocol_version, request_id),
        };
        if version != CONTROL_PROTOCOL_VERSION {
            return Err(ProtocolError::VersionMismatch {
                expected: CONTROL_PROTOCOL_VERSION,
                received: version,
            });
        }
        validate_id("request_id", request_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerEvent {
    pub protocol_version: u32,
    pub binding: SessionBinding,
    pub run_id: String,
    pub sequence: u64,
    pub emitted_at_ms: u64,
    #[serde(flatten)]
    pub payload: WorkerEventPayload,
}

impl WorkerEvent {
    pub fn kind(&self) -> &'static str {
        self.payload.kind()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum WorkerEventPayload {
    Started {
        agent_run_id: String,
        task: String,
        model: String,
    },
    Step {
        step: u32,
    },
    Reasoning {
        step: u32,
        text: String,
    },
    AssistantText {
        step: u32,
        text: String,
    },
    ToolCall {
        call_id: String,
        step: u32,
        sequence_in_step: u32,
        name: String,
        input: Value,
        repeat_count: u32,
    },
    ToolProgress {
        call_id: String,
        step: u32,
        sequence_in_step: u32,
        name: String,
        progress: Value,
    },
    ToolResult {
        call_id: String,
        step: u32,
        sequence_in_step: u32,
        name: String,
        input: Value,
        content: Value,
        summary: String,
        duration_ms: u64,
        error: Option<String>,
    },
    Evidence {
        step: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_call_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_tool: Option<String>,
        records: Vec<Value>,
    },
    ApiError {
        step: u32,
        message: String,
    },
    Done {
        steps: u32,
        final_text: String,
        usage: Value,
    },
    Failed {
        message: String,
    },
    Cancelled {
        reason: String,
    },
}

impl WorkerEventPayload {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Started { .. } => "started",
            Self::Step { .. } => "step",
            Self::Reasoning { .. } => "reasoning",
            Self::AssistantText { .. } => "assistant_text",
            Self::ToolCall { .. } => "tool_call",
            Self::ToolProgress { .. } => "tool_progress",
            Self::ToolResult { .. } => "tool_result",
            Self::Evidence { .. } => "evidence",
            Self::ApiError { .. } => "api_error",
            Self::Done { .. } => "done",
            Self::Failed { .. } => "failed",
            Self::Cancelled { .. } => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum WorkerOutput {
    Ready {
        protocol_version: u32,
        worker_instance_id: String,
        worker_version: String,
        socai_core_version: String,
        socai_core_revision: String,
        capabilities: Vec<String>,
    },
    Bound {
        protocol_version: u32,
        request_id: String,
        binding: SessionBinding,
        reused: bool,
    },
    RunAccepted {
        protocol_version: u32,
        request_id: String,
        run_id: String,
    },
    CommandAccepted {
        protocol_version: u32,
        request_id: String,
    },
    Event {
        event: Box<WorkerEvent>,
    },
    CommandError {
        protocol_version: u32,
        request_id: Option<String>,
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ProtocolError {
    #[error("protocol version mismatch: expected {expected}, received {received}")]
    VersionMismatch { expected: u32, received: u32 },
    #[error("invalid {field}: {message}")]
    InvalidField {
        field: &'static str,
        message: String,
    },
}

pub struct EventSequencer {
    binding: SessionBinding,
    run_id: String,
    next_sequence: u64,
    terminated: bool,
}

impl EventSequencer {
    pub fn new(binding: SessionBinding, run_id: impl Into<String>) -> Self {
        Self {
            binding,
            run_id: run_id.into(),
            next_sequence: 1,
            terminated: false,
        }
    }

    pub fn from_next_sequence(
        binding: SessionBinding,
        run_id: impl Into<String>,
        next_sequence: u64,
    ) -> Self {
        Self {
            binding,
            run_id: run_id.into(),
            next_sequence: next_sequence.max(1),
            terminated: false,
        }
    }

    pub fn event(&mut self, payload: WorkerEventPayload) -> Option<WorkerEvent> {
        if self.terminated {
            return None;
        }
        Some(self.next(payload))
    }

    pub fn evidence(
        &mut self,
        step: u32,
        source_call_id: Option<String>,
        source_tool: Option<String>,
        records: Vec<Value>,
    ) -> Option<WorkerEvent> {
        if self.terminated || records.is_empty() {
            return None;
        }
        Some(self.next(WorkerEventPayload::Evidence {
            step,
            source_call_id,
            source_tool,
            records,
        }))
    }

    pub fn done(&mut self, steps: u32, final_text: String, usage: Value) -> Option<WorkerEvent> {
        self.terminal(WorkerEventPayload::Done {
            steps,
            final_text,
            usage,
        })
    }

    pub fn failed(&mut self, message: impl Into<String>) -> Option<WorkerEvent> {
        self.terminal(WorkerEventPayload::Failed {
            message: message.into(),
        })
    }

    pub fn cancelled(&mut self, reason: impl Into<String>) -> Option<WorkerEvent> {
        self.terminal(WorkerEventPayload::Cancelled {
            reason: reason.into(),
        })
    }

    pub fn is_terminal(&self) -> bool {
        self.terminated
    }

    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    fn next(&mut self, payload: WorkerEventPayload) -> WorkerEvent {
        let event = WorkerEvent {
            protocol_version: EVENT_SCHEMA_VERSION,
            binding: self.binding.clone(),
            run_id: self.run_id.clone(),
            sequence: self.next_sequence,
            emitted_at_ms: now_ms(),
            payload,
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        event
    }

    fn terminal(&mut self, payload: WorkerEventPayload) -> Option<WorkerEvent> {
        if self.terminated {
            return None;
        }
        self.terminated = true;
        Some(self.next(payload))
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    fn binding() -> SessionBinding {
        SessionBinding {
            user_id: "user-1".into(),
            session_id: "session-1".into(),
            kernel_id: "kernel-1".into(),
            assignment_epoch: 4,
        }
    }

    #[test]
    fn protocol_versions_are_stable() {
        assert_eq!(CONTROL_PROTOCOL_VERSION, 2);
        assert_eq!(EVENT_SCHEMA_VERSION, 1);
    }

    #[test]
    fn sequences_wire_events_and_enriched_terminals() {
        let mut sequencer = EventSequencer::new(binding(), "run-1");
        let inputs = vec![
            WorkerEventPayload::Started {
                agent_run_id: "core-run".into(),
                task: "research".into(),
                model: "model".into(),
            },
            WorkerEventPayload::Step { step: 1 },
            WorkerEventPayload::Reasoning {
                step: 1,
                text: "reason".into(),
            },
            WorkerEventPayload::AssistantText {
                step: 1,
                text: "answer".into(),
            },
            WorkerEventPayload::ToolCall {
                call_id: "call-1".into(),
                step: 1,
                sequence_in_step: 1,
                name: "search".into(),
                input: json!({"query":"wearables"}),
                repeat_count: 1,
            },
            WorkerEventPayload::ToolProgress {
                call_id: "call-1".into(),
                step: 1,
                sequence_in_step: 1,
                name: "search".into(),
                progress: json!({
                    "phase":"reading", "status":"item_completed", "current":1,
                    "total":2, "item_index":1, "title":"post"
                }),
            },
            WorkerEventPayload::ToolResult {
                call_id: "call-1".into(),
                step: 1,
                sequence_in_step: 1,
                name: "search".into(),
                input: json!({"query":"wearables"}),
                content: json!({"sources":1}),
                summary: "one source".into(),
                duration_ms: 20,
                error: None,
            },
            WorkerEventPayload::ApiError {
                step: 2,
                message: "retry".into(),
            },
        ];
        let events: Vec<_> = inputs
            .into_iter()
            .filter_map(|event| sequencer.event(event))
            .collect();
        assert_eq!(
            events.iter().map(WorkerEvent::kind).collect::<Vec<_>>(),
            vec![
                "started",
                "step",
                "reasoning",
                "assistant_text",
                "tool_call",
                "tool_progress",
                "tool_result",
                "api_error",
            ]
        );
        assert_eq!(
            events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            (1..=8).collect::<Vec<_>>()
        );
        let Some(done) = sequencer.done(2, "final".into(), json!({"input_tokens":1})) else {
            panic!("first terminal event was suppressed");
        };
        let failed = sequencer.failed("failed");
        let cancelled = sequencer.cancelled("cancelled by user");
        assert_eq!(done.sequence, 9);
        assert_eq!(done.kind(), "done");
        assert!(sequencer.is_terminal());
        assert!(failed.is_none());
        assert!(cancelled.is_none());
    }

    #[test]
    fn emits_core_note_archive_evidence_as_an_ordered_event() {
        let mut sequencer = EventSequencer::new(binding(), "run-1");
        let event = sequencer
            .evidence(
                1,
                Some("call-1".into()),
                Some("search".into()),
                vec![json!({
                    "note_id":"xhs:note-1",
                    "url":"https://www.xiaohongshu.com/explore/note-1",
                    "title":"AI wearable field test"
                })],
            )
            .expect("evidence event");

        let WorkerEventPayload::Evidence { records, .. } = event.payload else {
            panic!("expected evidence");
        };
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["note_id"], "xhs:note-1");
    }

    #[test]
    fn evidence_event_stays_below_the_backend_frame_limit() {
        let mut sequencer = EventSequencer::new(binding(), "run-1");
        let records = (0..3)
            .map(|index| {
                json!({
                    "note_id":format!("xhs:note-{index}"),
                    "url":format!("https://www.xiaohongshu.com/explore/note-{index}"),
                    "content":"x".repeat(128 * 1024)
                })
            })
            .collect();
        let event = sequencer
            .evidence(1, Some("call-1".into()), Some("search".into()), records)
            .expect("evidence event");
        let encoded = serde_json::to_vec(&WorkerOutput::Event {
            event: Box::new(event),
        })
        .expect("serialize evidence frame");

        assert!(encoded.len() < 1024 * 1024);
    }

    #[test]
    fn validates_version_binding_and_limits() -> Result<(), ProtocolError> {
        let command = WorkerCommand::SessionBind {
            protocol_version: CONTROL_PROTOCOL_VERSION,
            request_id: "request-1".into(),
            binding: binding(),
        };
        command.validate()?;

        let bad = WorkerCommand::RunCancel {
            protocol_version: CONTROL_PROTOCOL_VERSION + 1,
            request_id: "request-2".into(),
            binding: binding(),
            run_id: "run-1".into(),
        };
        assert!(matches!(
            bad.validate(),
            Err(ProtocolError::VersionMismatch { .. })
        ));
        WorkerCommand::RunResume {
            protocol_version: CONTROL_PROTOCOL_VERSION,
            request_id: "request-3".into(),
            binding: binding(),
            run_id: "run-1".into(),
        }
        .validate()?;
        Ok(())
    }

    #[test]
    fn serializes_one_ndjson_safe_event_frame() -> Result<(), Box<dyn std::error::Error>> {
        let mut sequencer = EventSequencer::new(binding(), "run-1");
        let event = sequencer
            .event(WorkerEventPayload::AssistantText {
                step: 1,
                text: "line one\nline two".into(),
            })
            .ok_or("expected assistant event")?;
        let encoded = serde_json::to_string(&WorkerOutput::Event {
            event: Box::new(event),
        })?;
        assert_eq!(encoded.lines().count(), 1);
        let decoded: WorkerOutput = serde_json::from_str(&encoded)?;
        assert!(matches!(decoded, WorkerOutput::Event { .. }));
        Ok(())
    }
}
