//! Copilot in VS Code, ingested from both places VS Code keeps its chats: its own chat session
//! files for the Local harness, and Copilot's session files for the Copilot harness.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use agent_dashboard_core::agent::Agent;
use agent_dashboard_core::history::History;
use agent_dashboard_core::ingest::{self, Report};
use agent_dashboard_core::pricing::PriceTable;
use agent_dashboard_core::report::{self, Filter, Range};
use agent_dashboard_core::roots::DataRoot;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};

const SESSION: &str = "5463ddbb-3b10-4327-8522-b7a30b4c980a";

/// From the start of 2026 to the end of 2027: every request below.
const ALL: Range = Range {
    from_ms: 1_767_225_600_000,
    to_ms: 1_830_297_600_000,
};

/// A prompt as VS Code saves it, with made-up content; `None` credits for one VS Code has not
/// charged, and a running prompt has no completion time.
fn prompt(id: &str, tokens: u64, credits: Option<f64>, finished: bool) -> Value {
    let mut prompt = json!({
        "requestId": format!("request_{id}"),
        "timestamp": 1_793_000_000_000_i64,
        "modelId": "copilot/claude-sonnet-4.6",
        "agent": {"id": "github.copilot.editsAgent", "extensionVersion": "0.73.0"},
        "message": {"text": "Made-up prompt.", "parts": []},
        "response": [{"kind": "markdownContent", "value": "Made-up reply."}],
        "modelState": if finished {
            json!({"value": 1, "completedAt": 1_793_000_060_000_i64})
        } else {
            json!({"value": 0})
        },
        "completionTokens": tokens,
        "result": {"metadata": {"toolCallRounds": [{"modelId": "claude-sonnet-4.6"}]}}
    });
    if let Some(credits) = credits {
        prompt["copilotCredits"] = json!(credits);
    }
    prompt
}

fn line(entry: &Value) -> Vec<u8> {
    let mut line = serde_json::to_vec(entry).unwrap();
    line.push(b'\n');
    line
}

fn whole(prompts: &[Value]) -> Vec<u8> {
    line(&json!({
        "kind": 0,
        "v": {"version": 3, "sessionId": SESSION, "customTitle": "Fix the importer", "requests": prompts}
    }))
}

fn append(path: &Path, bytes: &[u8]) {
    OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}

struct Fixture {
    vscode: PathBuf,
    copilot: PathBuf,
    history: History,
    /// Last, so that the history in it closes first.
    _dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let history = History::open(&dir.path().join("history.sqlite")).unwrap();
        Self {
            vscode: dir.path().join("Code"),
            copilot: dir.path().join(".copilot"),
            history,
            _dir: dir,
        }
    }

    /// A chat session file in the storage folder of a workspace whose folder is `C:\code\app`.
    fn chat_session(&self) -> PathBuf {
        let workspace = self.vscode.join("User/workspaceStorage/0f3a");
        fs::create_dir_all(workspace.join("chatSessions")).unwrap();
        fs::write(
            workspace.join("workspace.json"),
            r#"{"folder":"file:///c%3A/code/app"}"#,
        )
        .unwrap();
        workspace
            .join("chatSessions")
            .join(format!("{SESSION}.jsonl"))
    }

    fn ingest(&mut self) -> Report {
        ingest::ingest(
            &mut self.history,
            &[
                DataRoot::added(&self.copilot, Agent::CopilotCli),
                DataRoot::added(&self.vscode, Agent::CopilotVscode),
            ],
        )
        .unwrap()
        .expect("no other process holds the ingest lock")
    }

    /// Every request as (record id, output tokens, charge in AI credits, per prompt).
    fn requests(&self) -> Vec<(String, i64, f64, bool)> {
        let conn =
            Connection::open_with_flags(self.history.path(), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        conn.prepare(
            "SELECT record_id, output_tokens, coalesce(charge_input_nano, 0) / 1e9, per_prompt
             FROM requests ORDER BY record_id",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    }
}

#[test]
fn a_chat_session_file_is_read_whole_whenever_it_changes_and_its_prompts_stay() {
    let mut fixture = Fixture::new();
    let log = fixture.chat_session();
    fs::write(
        &log,
        whole(&[
            prompt("1", 100, Some(2.0), true),
            prompt("2", 0, None, false),
        ]),
    )
    .unwrap();

    let report = fixture.ingest();
    assert_eq!((report.request_records, report.requests_changed), (1, 1));
    assert_eq!(
        fixture.requests(),
        [("vscode:request_1".to_owned(), 100, 2.0, true)]
    );
    let session = fixture.history.session(SESSION).unwrap().unwrap();
    assert_eq!(session.agent, Agent::CopilotVscode);
    assert_eq!(session.title.as_deref(), Some("Fix the importer"));
    let projects = fixture.history.projects().unwrap();
    let project = projects.iter().find(|p| p.key == session.project).unwrap();
    assert_eq!(project.path.as_deref(), Some(r"C:\code\app"));

    let unchanged = fixture.ingest();
    assert_eq!(
        (unchanged.transcripts_unchanged, unchanged.requests_changed),
        (1, 0)
    );

    // The running prompt finishes, and the first one's subagents add credits but no output.
    let set = |path: Value, value: Value| line(&json!({"kind": 1, "k": path, "v": value}));
    append(
        &log,
        &set(json!(["requests", 1, "completionTokens"]), json!(50)),
    );
    append(
        &log,
        &set(json!(["requests", 1, "copilotCredits"]), json!(1.5)),
    );
    append(
        &log,
        &set(
            json!(["requests", 1, "modelState"]),
            json!({"value": 1, "completedAt": 1_793_000_120_000_i64}),
        ),
    );
    append(
        &log,
        &set(json!(["requests", 0, "copilotCredits"]), json!(2.5)),
    );
    let report = fixture.ingest();
    assert_eq!((report.request_records, report.requests_changed), (2, 2));
    assert_eq!(
        fixture.requests(),
        [
            ("vscode:request_1".to_owned(), 100, 2.5, true),
            ("vscode:request_2".to_owned(), 50, 1.5, true),
        ]
    );

    // The person edits the first prompt away and sends another, then VS Code writes the file
    // again from a fresh first line: the charged prompt stays.
    append(
        &log,
        &line(
            &json!({"kind": 2, "k": ["requests"], "v": [prompt("3", 10, Some(0.4), true)], "i": 0}),
        ),
    );
    fixture.ingest();
    fs::write(
        &log,
        whole(&[
            prompt("2", 50, Some(1.5), true),
            prompt("3", 10, Some(0.4), true),
        ]),
    )
    .unwrap();
    let report = fixture.ingest();
    assert_eq!(report.requests_changed, 0);
    let ids: Vec<String> = fixture.requests().into_iter().map(|r| r.0).collect();
    assert_eq!(
        ids,
        ["vscode:request_1", "vscode:request_2", "vscode:request_3"]
    );

    let totals = report::totals(
        &fixture.history,
        PriceTable::bundled(),
        ALL,
        &Filter::agent(Agent::CopilotVscode),
    )
    .unwrap();
    assert_eq!(totals.requests, 3);
    assert_eq!(totals.per_prompt_requests, 3);
    assert_eq!(totals.charged_requests, 3);
    assert!((totals.cost.total() - 0.044).abs() < 1e-9, "{totals:?}");
}

#[test]
fn a_session_copied_to_another_workspace_counts_once() {
    let mut fixture = Fixture::new();
    let log = fixture.chat_session();
    let bytes = whole(&[prompt("1", 100, Some(2.0), true)]);
    fs::write(&log, &bytes).unwrap();
    let copy = fixture
        .vscode
        .join("User/globalStorage/emptyWindowChatSessions")
        .join(format!("{SESSION}.jsonl"));
    fs::create_dir_all(copy.parent().unwrap()).unwrap();
    fs::write(&copy, &bytes).unwrap();

    fixture.ingest();
    assert_eq!(fixture.requests().len(), 1);
}

#[test]
fn a_copilot_session_that_vs_code_started_is_copilot_in_vs_code_s() {
    let mut fixture = Fixture::new();
    let usage_record = |id: &str| {
        line(&json!({
            "type": "session.usage_record",
            "id": id,
            "timestamp": "2026-11-06T23:06:52.000Z",
            "data": {"usage": {
                "model": "claude-sonnet-4.6",
                "inputTokens": 30,
                "outputTokens": 5,
                "cacheReadTokens": 20,
                "accounting": {"usageId": id},
                "copilotUsage": {"tokenDetails": [], "totalNanoAiu": 4_400_000}
            }}
        }))
    };
    for (session, client) in [("cli-1", "github/cli"), ("vscode-1", "vscode-agent-host")] {
        let folder = fixture.copilot.join("session-state").join(session);
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join("workspace.yaml"),
            format!("id: {session}\ncwd: C:\\code\\app\nclient_name: {client}\n"),
        )
        .unwrap();
        let mut events = line(&json!({
            "type": "session.start",
            "data": {"copilotVersion": "1.0.106", "context": {"cwd": "C:\\code\\app"}}
        }));
        events.extend(usage_record(&format!("{session}-u1")));
        fs::write(folder.join("events.jsonl"), events).unwrap();
    }

    fixture.ingest();
    let agent_of = |session: &str| fixture.history.session(session).unwrap().unwrap().agent;
    assert_eq!(agent_of("cli-1"), Agent::CopilotCli);
    assert_eq!(agent_of("vscode-1"), Agent::CopilotVscode);
    assert!(
        fixture.requests().iter().all(|request| !request.3),
        "the Copilot harness records each model call"
    );
    let sessions = fixture.history.sessions().unwrap();
    assert_eq!(
        sessions
            .iter()
            .map(|session| session.project)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        1,
        "one project for the folder both agents worked in"
    );
}
