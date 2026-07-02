use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// Three-way root-cause classification, populated at incident-review time
/// (not at event-creation time — most events are never incidents).
/// `02-security-core.md` §1: standard app logs can't tell whether a bad
/// outcome was a hallucination, a successful injection, or privilege
/// drift; this field exists so post-incident forensics can say which.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CausalClass {
    Hallucination,
    Injection,
    PrivilegeDrift,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub actor: String,
    pub capability_kind: String,
    pub action: String,
    pub decision: String,
    /// Human-readable, for auditors/compliance.
    pub reasoning_summary: String,
    /// Structured, for engineers debugging — distinct from
    /// `reasoning_summary` per the FINOS AIR "Agent Decision Audit and
    /// Explainability" framing this schema aligns with.
    pub evidence_chain: serde_json::Value,
    pub causal_class: Option<CausalClass>,
}

impl AuditEvent {
    pub fn new(
        actor: impl Into<String>,
        capability_kind: impl Into<String>,
        action: impl Into<String>,
        decision: impl Into<String>,
        reasoning_summary: impl Into<String>,
        evidence_chain: serde_json::Value,
    ) -> Self {
        AuditEvent {
            id: Uuid::new_v4(),
            timestamp: OffsetDateTime::now_utc(),
            actor: actor.into(),
            capability_kind: capability_kind.into(),
            action: action.into(),
            decision: decision.into(),
            reasoning_summary: reasoning_summary.into(),
            evidence_chain,
            causal_class: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("failed to open audit log at {path}: {source}")]
    Open {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write audit event: {0}")]
    Write(#[source] std::io::Error),
    #[error("failed to read audit log: {0}")]
    Read(#[source] std::io::Error),
    #[error("failed to parse audit event: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Append-only, local-first JSONL audit store. No network egress for audit
/// data by default, consistent with the "local-first optional" pillar —
/// see `02-security-core.md` §1's storage/retention/privacy requirements.
pub struct AuditLog {
    path: PathBuf,
}

impl AuditLog {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        AuditLog { path: path.into() }
    }

    pub fn append(&self, event: &AuditEvent) -> Result<(), AuditError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|source| AuditError::Open {
                    path: self.path.display().to_string(),
                    source,
                })?;
            }
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| AuditError::Open { path: self.path.display().to_string(), source })?;
        let line = serde_json::to_string(event)?;
        writeln!(file, "{line}").map_err(AuditError::Write)?;
        Ok(())
    }

    /// Reads every event in the log. Backs the capability-inventory view
    /// (`02-security-core.md` §1) — a UI/query investment over data this
    /// store already collects, not new backend work.
    pub fn all(&self) -> Result<Vec<AuditEvent>, AuditError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = std::fs::File::open(&self.path).map_err(AuditError::Read)?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(AuditError::Read)?;
            if line.trim().is_empty() {
                continue;
            }
            events.push(serde_json::from_str(&line)?);
        }
        Ok(events)
    }

    pub fn query(&self, predicate: impl Fn(&AuditEvent) -> bool) -> Result<Vec<AuditEvent>, AuditError> {
        Ok(self.all()?.into_iter().filter(predicate).collect())
    }
}

/// Configurable, non-indefinite retention. `02-security-core.md` §1
/// requires this before any regulated-audience pack ships.
#[derive(Debug, Clone, Copy)]
pub struct RetentionPolicy {
    pub max_age_days: u32,
}

impl RetentionPolicy {
    pub fn is_expired(&self, event: &AuditEvent, now: OffsetDateTime) -> bool {
        let age = now - event.timestamp;
        age.whole_days() > i64::from(self.max_age_days)
    }
}

/// Redacts every occurrence of the given literal patterns from
/// `reasoning_summary`. A stub sufficient for Phase 0 — full secret/PII
/// detection is a later hardening pass, but the redaction *path* must
/// exist before this ships to any regulated audience.
pub fn redact(event: &AuditEvent, patterns: &[&str]) -> AuditEvent {
    let mut redacted = event.clone();
    for pattern in patterns {
        if pattern.is_empty() {
            continue;
        }
        redacted.reasoning_summary = redacted.reasoning_summary.replace(pattern, "[REDACTED]");
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_log_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("pro-audit-test-{name}-{nonce}.jsonl"))
    }

    #[test]
    fn append_then_query_round_trips() {
        let path = temp_log_path("roundtrip");
        let log = AuditLog::new(&path);
        let event = AuditEvent::new(
            "cli",
            "run",
            "execute echo",
            "Allowed",
            "user requested a shell echo",
            serde_json::json!({"argv": ["echo", "hi"]}),
        );
        log.append(&event).unwrap();

        let all = log.all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].action, "execute echo");
        assert_eq!(all[0].causal_class, None);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn query_filters_by_predicate() {
        let path = temp_log_path("query");
        let log = AuditLog::new(&path);
        log.append(&AuditEvent::new("cli", "fs_read", "read", "Allowed", "", serde_json::json!({}))).unwrap();
        log.append(&AuditEvent::new("cli", "run", "exec", "Denied", "", serde_json::json!({}))).unwrap();

        let denied = log.query(|e| e.decision == "Denied").unwrap();
        assert_eq!(denied.len(), 1);
        assert_eq!(denied[0].capability_kind, "run");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn redact_replaces_pattern_in_summary() {
        let event = AuditEvent::new("cli", "env", "read", "Allowed", "used token sk-abc123", serde_json::json!({}));
        let redacted = redact(&event, &["sk-abc123"]);
        assert!(!redacted.reasoning_summary.contains("sk-abc123"));
        assert!(redacted.reasoning_summary.contains("[REDACTED]"));
    }

    #[test]
    fn retention_policy_flags_old_events_expired() {
        let mut event = AuditEvent::new("cli", "fs_read", "read", "Allowed", "", serde_json::json!({}));
        event.timestamp = OffsetDateTime::now_utc() - time::Duration::days(100);
        let policy = RetentionPolicy { max_age_days: 90 };
        assert!(policy.is_expired(&event, OffsetDateTime::now_utc()));
    }
}
