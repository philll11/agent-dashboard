//! What the CLI's tests share: a data root of made-up transcripts and a history of their own, in
//! a folder that goes away after each test, and a fixed moment for every command to see.

#![allow(dead_code, reason = "each test file uses a part of these")]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{Value, json};

/// The moment every command the tests run sees, so that no test depends on when it runs: midday,
/// in UTC, on the price table's date.
pub const NOW: &str = "2026-10-10T12:00:00Z";
pub const TODAY: &str = "2026-10-10";

pub struct Fixture {
    pub dir: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    pub fn root(&self) -> PathBuf {
        self.dir.path().join(".claude")
    }

    /// Copilot CLI's data root, which stays empty unless a test writes to it, so that no test
    /// reads the Copilot CLI sessions of the computer it runs on.
    pub fn copilot_root(&self) -> PathBuf {
        self.dir.path().join(".copilot")
    }

    /// VS Code's user data folder, which stays empty unless a test writes to it, so that no test
    /// reads the VS Code chats of the computer it runs on.
    pub fn vscode_root(&self) -> PathBuf {
        self.dir.path().join("vscode")
    }

    pub fn history(&self) -> PathBuf {
        self.dir.path().join("history.sqlite")
    }

    /// The command, kept to this fixture's data root and history, in UTC and without colour.
    pub fn cli(&self, args: &[&str]) -> Command {
        self.cli_at(Path::new(env!("CARGO_BIN_EXE_agent-dashboard")), args)
    }

    /// The command as `cli` gives it, run from `program`.
    pub fn cli_at(&self, program: &Path, args: &[&str]) -> Command {
        let mut command = Command::new(program);
        command
            .args(args)
            .env("CLAUDE_CONFIG_DIR", self.root())
            .env("COPILOT_HOME", self.copilot_root())
            .env("AGENT_DASHBOARD_VSCODE_DATA", self.vscode_root())
            .env("AGENT_DASHBOARD_HISTORY", self.history())
            .env("AGENT_DASHBOARD_NOW", NOW)
            .env("TZ", "UTC")
            .env("NO_COLOR", "1")
            .env_remove("COLORTERM")
            .env_remove("WT_SESSION");
        command
    }

    /// Writes a session's transcript in the project `C:\code\app`.
    pub fn transcript(&self, session: &str, records: &[Value]) {
        let path = self
            .root()
            .join("projects")
            .join("C--code-app")
            .join(format!("{session}.jsonl"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let lines: Vec<String> = records.iter().map(Value::to_string).collect();
        fs::write(path, lines.join("\n") + "\n").unwrap();
    }

    /// Writes a Copilot CLI session called `title`, working in `C:\code\app`: its
    /// `workspace.yaml`, and its `events.jsonl` with the event that starts it, then `events`.
    pub fn copilot_session(&self, session: &str, title: &str, events: &[Value]) {
        let folder = self.copilot_root().join("session-state").join(session);
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join("workspace.yaml"),
            format!("id: {session}\ncwd: C:\\code\\app\nname: {title}\n"),
        )
        .unwrap();
        let start = json!({"type": "session.start",
            "data": {"copilotVersion": "1.0.106", "context": {"cwd": "C:\\code\\app"}}});
        let lines: Vec<String> = std::iter::once(&start)
            .chain(events)
            .map(Value::to_string)
            .collect();
        fs::write(folder.join("events.jsonl"), lines.join("\n") + "\n").unwrap();
    }

    /// The command with nothing on the `PATH`, so that `connect` names agent-dashboard by its
    /// full path on every platform.
    pub fn cli_without_path(&self, args: &[&str]) -> Command {
        let mut command = self.cli(args);
        command.env("PATH", self.dir.path().join("empty"));
        command
    }

    pub fn settings(&self) -> Value {
        serde_json::from_str(&fs::read_to_string(self.root().join("settings.json")).unwrap())
            .unwrap()
    }
}

/// An assistant record of `model` made at `timestamp`, costing $0.031712 on Claude Opus 5.5.
pub fn request(id: &str, model: &str, timestamp: &str) -> Value {
    json!({
        "type": "assistant", "uuid": format!("rec-{id}"), "requestId": format!("req_{id}"),
        "timestamp": timestamp, "cwd": "C:\\code\\app",
        "message": {"id": format!("msg_{id}"), "model": model, "content": [],
            "usage": {"input_tokens": 3, "output_tokens": 640, "cache_read_input_tokens": 42000,
                "cache_creation_input_tokens": 1500,
                "cache_creation": {"ephemeral_5m_input_tokens": 500, "ephemeral_1h_input_tokens": 1000}}}
    })
}

/// A Copilot CLI request of `model`, which Copilot CLI names `name`, made at `timestamp`: the
/// person started it, and Copilot CLI recorded it as costing 5 AI credits, $0.05.
pub fn copilot_request(id: &str, (model, name): (&str, &str), timestamp: &str) -> Value {
    json!({
        "type": "session.usage_record", "id": format!("event-{id}"), "timestamp": timestamp,
        "data": {"usage": {"model": model, "modelDisplayName": name,
            "inputTokens": 1000, "outputTokens": 200,
            "cacheReadTokens": 0, "cacheWriteTokens": 0, "reasoningTokens": 50,
            "initiator": "user", "isByok": false,
            "accounting": {"usageId": format!("msg_{id}")},
            "copilotUsage": {"tokenDetails": [{"batchSize": 1000, "costPerBatch": 5_000_000_000_u64,
                "tokenCount": 1000, "tokenType": "input"}], "totalNanoAiu": 5_000_000_000_u64}}}
    })
}

/// Copilot CLI's monthly allowance as a model call at `timestamp` reports it: 1,575 of 45,000 AI
/// credits used, until 1 November 2026.
pub fn copilot_allowance(timestamp: &str) -> Value {
    json!({
        "type": "model.model_call_success", "timestamp": timestamp,
        "data": {"quotaSnapshots": {"premium_interactions": {"isUnlimitedEntitlement": false,
            "entitlementRequests": 45000, "usedRequests": 1575, "remainingPercentage": 96.5,
            "resetDate": "2026-11-01T00:00:00Z"}}}
    })
}

pub fn today_at(time: &str) -> String {
    format!("{TODAY}T{time}Z")
}
