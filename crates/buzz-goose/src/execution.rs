use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::process::Command;
use uuid::Uuid;

/// Shared task state; success is possible only after verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionState {
    Draft,
    Planning,
    AwaitingApproval,
    Queued,
    Running,
    Paused,
    Verifying,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
}

/// Capability granted only for this execution.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    RepositoryRead,
    WorkspaceWrite,
    Network,
    ToolExecution,
    ExternalEffect,
}

/// Permission denied independently of a runtime's available tools.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Permission {
    Read,
    Write,
    Execute,
    Network,
    ExternalPublish,
}

/// Inspectable agent composition, preserving lineage and trust boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentGenome {
    pub agent_id: String,
    pub version: String,
    pub runtime: String,
    pub lineage: Vec<String>,
    pub capabilities: BTreeSet<Capability>,
    pub provenance: String,
}

/// Least-privilege contract supplied to an individual runtime invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionEnvelope {
    pub execution_id: Uuid,
    pub tenant_id: String,
    pub actor_id: String,
    pub goal: String,
    pub plan_id: String,
    pub step_id: String,
    pub agent: AgentGenome,
    pub workspace: PathBuf,
    pub allowed_capabilities: BTreeSet<Capability>,
    pub denied_permissions: BTreeSet<Permission>,
    pub secret_references: Vec<String>,
    pub runtime_limit_secs: u64,
    pub spending_limit_usd: f64,
    pub approval_required: bool,
    pub revoked: bool,
    pub verification_requirements: Vec<String>,
}

/// Timestamped state transition retained as evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionEvent {
    pub at: DateTime<Utc>,
    pub state: ExecutionState,
    pub message: String,
}

/// Independent checks performed after Goose returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub passed: bool,
    pub checks: Vec<String>,
    pub failures: Vec<String>,
}

/// Evidence record for an actual Goose attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecord {
    pub envelope: ExecutionEnvelope,
    pub state: ExecutionState,
    pub runtime: GooseHealth,
    pub events: Vec<ExecutionEvent>,
    pub output: String,
    pub output_sha256: String,
    pub verification: VerificationResult,
    pub receipt_sha256: String,
}

/// Real Goose availability discovered through its documented CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GooseHealth {
    pub available: bool,
    pub version: Option<String>,
    pub diagnostic: Option<String>,
}

/// Input for the first supported real vertical slice.
#[derive(Debug, Clone)]
pub struct RepositoryHealthRequest {
    pub workspace: PathBuf,
    pub goal: String,
    pub tenant_id: String,
    pub actor_id: String,
}

/// Deployment supplied Goose executable configuration.
#[derive(Debug, Clone)]
pub struct GooseRuntimeConfig {
    pub executable: PathBuf,
    pub timeout: Duration,
    pub max_turns: u32,
}

impl GooseRuntimeConfig {
    /// Loads `BUZZ_GOOSE_PATH`; paths are never guessed or embedded in source.
    pub fn from_env() -> Result<Self, String> {
        let executable = std::env::var_os("BUZZ_GOOSE_PATH")
            .map(PathBuf::from)
            .ok_or_else(|| "BUZZ_GOOSE_PATH is not configured".to_string())?;
        Ok(Self {
            executable,
            timeout: Duration::from_secs(180),
            max_turns: 8,
        })
    }
}

/// Adapter that invokes Goose without modifying Goose internals.
#[derive(Debug, Clone)]
pub struct GooseRuntime {
    config: GooseRuntimeConfig,
}

impl GooseRuntime {
    /// Builds an adapter around the deployment-approved Goose executable.
    pub fn new(config: GooseRuntimeConfig) -> Self {
        Self { config }
    }

    /// Checks availability using Goose's supported `--version` command.
    pub async fn health(&self) -> GooseHealth {
        match Command::new(&self.config.executable)
            .arg("--version")
            .output()
            .await
        {
            Ok(output) if output.status.success() => GooseHealth {
                available: true,
                version: Some(String::from_utf8_lossy(&output.stdout).trim().to_string()),
                diagnostic: None,
            },
            Ok(output) => GooseHealth {
                available: false,
                version: None,
                diagnostic: Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
            },
            Err(error) => GooseHealth {
                available: false,
                version: None,
                diagnostic: Some(error.to_string()),
            },
        }
    }

    /// Executes real bounded Goose repository inspection and independently verifies it.
    pub async fn inspect_repository_health(
        &self,
        request: RepositoryHealthRequest,
    ) -> ExecutionRecord {
        let envelope = envelope_for(&request);
        let runtime = self.health().await;
        let mut events = vec![
            event(ExecutionState::Draft, "Repository-health goal received."),
            event(ExecutionState::Planning, "Built bounded read-only plan."),
        ];
        if let Err(reason) = validate(&envelope) {
            events.push(event(ExecutionState::Blocked, reason.clone()));
            return record(
                envelope,
                ExecutionState::Blocked,
                runtime,
                events,
                String::new(),
                VerificationResult {
                    passed: false,
                    checks: vec![],
                    failures: vec![reason],
                },
            );
        }
        if !runtime.available {
            let reason = format!(
                "BLOCKED: Goose runtime unavailable{}",
                runtime
                    .diagnostic
                    .as_ref()
                    .map(|v| format!(": {v}"))
                    .unwrap_or_default()
            );
            events.push(event(ExecutionState::Blocked, reason.clone()));
            return record(
                envelope,
                ExecutionState::Blocked,
                runtime,
                events,
                String::new(),
                VerificationResult {
                    passed: false,
                    checks: vec![],
                    failures: vec![reason],
                },
            );
        }
        let before = git_status(&envelope.workspace).await;
        events.push(event(
            ExecutionState::Queued,
            "Policy preflight passed; Goose worker admitted.",
        ));
        events.push(event(
            ExecutionState::Running,
            "Goose repository worker executing bounded inspection.",
        ));
        let prompt = "Inspect this repository for health. Read files and repository metadata only. Do not edit, create, delete, stage, commit, fetch, push, install dependencies, start services, send network requests, or use credentials. Report repository status, detected stack, documented test commands, obvious risks, uncertainty, and a concise evidence-based health assessment.";
        let command = Command::new(&self.config.executable)
            .current_dir(&envelope.workspace)
            .args([
                "run",
                "--no-session",
                "--no-profile",
                "--with-builtin",
                "developer",
                "--max-turns",
            ])
            .arg(self.config.max_turns.to_string())
            .args([
                "--max-tool-repetitions",
                "2",
                "--output-format",
                "json",
                "--text",
                prompt,
            ])
            .output();
        let (success, output, stderr) =
            match tokio::time::timeout(self.config.timeout, command).await {
                Ok(Ok(result)) => (
                    result.status.success(),
                    String::from_utf8_lossy(&result.stdout).to_string(),
                    String::from_utf8_lossy(&result.stderr).trim().to_string(),
                ),
                Ok(Err(error)) => (
                    false,
                    String::new(),
                    format!("Goose process could not start: {error}"),
                ),
                Err(_) => (
                    false,
                    String::new(),
                    "Goose execution exceeded configured runtime limit.".to_string(),
                ),
            };
        events.push(event(
            ExecutionState::Verifying,
            "Independent verifier checking output and worktree state.",
        ));
        let after = git_status(&envelope.workspace).await;
        let verification = verify(
            success,
            &output,
            &stderr,
            before.as_deref(),
            after.as_deref(),
        );
        let state = if verification.passed {
            ExecutionState::Succeeded
        } else {
            ExecutionState::Failed
        };
        events.push(event(
            state,
            if verification.passed {
                "Verification passed; evidence receipt created."
            } else {
                "Verification failed; output retained as evidence."
            },
        ));
        record(envelope, state, runtime, events, output, verification)
    }
}

/// Artifact destination that persists actual immutable execution records.
pub trait ArtifactStore {
    fn store(&self, record: &ExecutionRecord) -> Result<String, String>;
}

/// Local durable artifact store for a worker plane; not a replacement for Buzz Postgres indexes.
#[derive(Debug, Clone)]
pub struct FileArtifactStore {
    root: PathBuf,
}
impl FileArtifactStore {
    /// Creates a store in an approved worker-plane directory.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}
impl ArtifactStore for FileArtifactStore {
    fn store(&self, record: &ExecutionRecord) -> Result<String, String> {
        std::fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let name = format!("{}.json", record.envelope.execution_id);
        let path = self.root.join(name);
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(record).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        Ok(path.to_string_lossy().to_string())
    }
}

fn envelope_for(request: &RepositoryHealthRequest) -> ExecutionEnvelope {
    let capabilities = BTreeSet::from([Capability::RepositoryRead]);
    ExecutionEnvelope {
        execution_id: Uuid::new_v4(),
        tenant_id: request.tenant_id.clone(),
        actor_id: request.actor_id.clone(),
        goal: request.goal.clone(),
        plan_id: "repository-health-v1".to_string(),
        step_id: "inspect".to_string(),
        agent: AgentGenome {
            agent_id: "goose.repository-health".to_string(),
            version: "1.0.0".to_string(),
            runtime: "goose-cli".to_string(),
            lineage: vec![
                "Buzz".to_string(),
                "Goose".to_string(),
                "Universal Operator envelope".to_string(),
            ],
            capabilities: capabilities.clone(),
            provenance: "local Buzz Goose adapter".to_string(),
        },
        workspace: request.workspace.clone(),
        allowed_capabilities: capabilities,
        denied_permissions: BTreeSet::from([
            Permission::Write,
            Permission::Network,
            Permission::ExternalPublish,
        ]),
        secret_references: vec![],
        runtime_limit_secs: 180,
        spending_limit_usd: 0.0,
        approval_required: false,
        revoked: false,
        verification_requirements: vec![
            "goose-exit-status".to_string(),
            "non-empty-output".to_string(),
            "worktree-unchanged".to_string(),
        ],
    }
}

fn validate(envelope: &ExecutionEnvelope) -> Result<(), String> {
    if envelope.revoked {
        return Err("agent is revoked or quarantined".to_string());
    }
    if envelope.allowed_capabilities != BTreeSet::from([Capability::RepositoryRead]) {
        return Err("repository-health permits only repository-read capability".to_string());
    }
    for permission in [
        Permission::Write,
        Permission::Network,
        Permission::ExternalPublish,
    ] {
        if !envelope.denied_permissions.contains(&permission) {
            return Err(
                "repository-health must deny write, network, and external-publish".to_string(),
            );
        }
    }
    if !envelope.workspace.is_dir() || !envelope.workspace.join(".git").exists() {
        return Err("workspace must be an existing Git repository".to_string());
    }
    Ok(())
}

fn event(state: ExecutionState, message: impl Into<String>) -> ExecutionEvent {
    ExecutionEvent {
        at: Utc::now(),
        state,
        message: message.into(),
    }
}
fn sha256(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
fn record(
    envelope: ExecutionEnvelope,
    state: ExecutionState,
    runtime: GooseHealth,
    events: Vec<ExecutionEvent>,
    output: String,
    verification: VerificationResult,
) -> ExecutionRecord {
    let output_sha256 = sha256(&output);
    let receipt = serde_json::json!({ "envelope": &envelope, "state": state, "runtime": &runtime, "events": &events, "output_sha256": &output_sha256, "verification": &verification });
    let receipt_sha256 = sha256(&serde_json::to_string(&receipt).unwrap_or_default());
    ExecutionRecord {
        envelope,
        state,
        runtime,
        events,
        output,
        output_sha256,
        verification,
        receipt_sha256,
    }
}
async fn git_status(workspace: &Path) -> Option<String> {
    let result = Command::new("git")
        .arg("status")
        .arg("--porcelain=v1")
        .current_dir(workspace)
        .output()
        .await
        .ok()?;
    result
        .status
        .success()
        .then(|| String::from_utf8_lossy(&result.stdout).to_string())
}
fn verify(
    success: bool,
    output: &str,
    stderr: &str,
    before: Option<&str>,
    after: Option<&str>,
) -> VerificationResult {
    let mut checks = vec![];
    let mut failures = vec![];
    if success {
        checks.push("Goose process exited successfully.".to_string());
    } else {
        failures.push(format!("Goose process failed: {stderr}"));
    }
    if output.trim().is_empty() {
        failures.push("Goose returned no output.".to_string());
    } else {
        checks.push("Goose returned non-empty output.".to_string());
    }
    match (before, after) {
        (Some(before), Some(after)) if before == after => {
            checks.push("Git worktree status is unchanged.".to_string())
        }
        (Some(_), Some(_)) => {
            failures.push("Git worktree status changed during read-only execution.".to_string())
        }
        _ => failures.push("Could not capture Git worktree status.".to_string()),
    }
    VerificationResult {
        passed: failures.is_empty(),
        checks,
        failures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hash_is_deterministic() {
        assert_eq!(sha256("evidence"), sha256("evidence"));
        assert_ne!(sha256("evidence"), sha256("different"));
    }
    #[test]
    fn verification_rejects_worktree_change() {
        assert!(!verify(true, "finding", "", Some(""), Some(" M src/lib.rs\n")).passed);
    }
    #[test]
    fn envelope_is_read_only() {
        let envelope = envelope_for(&RepositoryHealthRequest {
            workspace: PathBuf::from("."),
            goal: "inspect".to_string(),
            tenant_id: "tenant".to_string(),
            actor_id: "actor".to_string(),
        });
        assert_eq!(
            envelope.allowed_capabilities,
            BTreeSet::from([Capability::RepositoryRead])
        );
        assert!(envelope.denied_permissions.contains(&Permission::Write));
    }
}
