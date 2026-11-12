//! Copilot's sessions: finding them in Copilot CLI's data root, and reading what each records.
//!
//! Copilot CLI keeps each session in a folder of its own under `session-state`: `events.jsonl`,
//! one event per line, which is the session's transcript, and `workspace.yaml`, which names the
//! session's working folder, title and the client that started it. The GitHub Copilot app keeps
//! its sessions there too, and so does VS Code for the chats it runs on its Copilot harness, which
//! belong to Copilot in VS Code (ADR 0017). GitHub documents where these files are but not their
//! format, so reading is tolerant: unknown events and fields are ignored, and a line that cannot be
//! read is counted and skipped (ADR 0016).

use std::fs;
use std::io::{BufRead, BufReader, Read as _};
use std::path::{Component, Path};
use std::time::{SystemTime, UNIX_EPOCH};

use memchr::memmem;
use serde::Deserialize;
use yaml_rust2::{Yaml, YamlLoader};

use crate::agent::Agent;
use crate::lenient::{lenient, lenient_list, object};
use crate::limits::{LimitSnapshot, MONTHLY, Source};
use crate::pricing::Charge;
use crate::tokens::Tokens;
use crate::transcript::{
    ModelUsage, Record, RequestRecord, SessionDetails, Transcript, Unreadable, open_shared,
};

/// The folder in Copilot CLI's data root that holds a folder for each session.
pub const SESSIONS: &str = "session-state";

/// A session's transcript, in its folder.
const EVENTS: &str = "events.jsonl";

/// A session's working folder and title, in its folder.
const WORKSPACE: &str = "workspace.yaml";

/// The most of `workspace.yaml`, or of the first event, that is read for a session's details.
const MAX_DETAILS: u64 = 64 * 1024;

/// The clients that name a session VS Code started: its agent host, which runs its Copilot
/// harness, and its earlier Copilot CLI integration.
const VSCODE_CLIENTS: [&str; 2] = ["vscode-agent-host", "vscode"];

/// Whether VS Code started the session that `details` describe, so that it belongs to Copilot in
/// VS Code rather than to Copilot CLI.
#[must_use]
pub fn started_by_vscode(details: &SessionDetails) -> bool {
    details
        .client
        .as_deref()
        .is_some_and(|client| VSCODE_CLIENTS.contains(&client))
}

/// Every session transcript in `root`'s `session-state` folder. A folder without one, such as
/// a session that was opened but never used, is no session yet.
///
/// A transcript counts as changed when its session's details change, so that a title given
/// after the last event is read too.
#[must_use]
pub fn find(root: &Path) -> Vec<Transcript> {
    let Ok(entries) = fs::read_dir(root.join(SESSIONS)) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let session_id = entry.file_name().to_str()?.to_owned();
            // Copilot CLI keeps its own locks in folders such as `.session-operation-locks`.
            if session_id.starts_with('.') {
                return None;
            }
            let folder = entry.path();
            let events = folder.join(EVENTS);
            let metadata = fs::metadata(&events).ok().filter(fs::Metadata::is_file)?;
            let changed = |metadata: &fs::Metadata| metadata.modified().unwrap_or(UNIX_EPOCH);
            let details: SystemTime = fs::metadata(folder.join(WORKSPACE))
                .as_ref()
                .map_or(UNIX_EPOCH, changed);
            Some(Transcript {
                path: events,
                agent: Agent::CopilotCli,
                project_folder: String::new(),
                session_id,
                subagent_id: None,
                len: metadata.len(),
                modified: changed(&metadata).max(details),
            })
        })
        .collect()
}

/// Whether a file, given relative to a data root's `session-state` folder, is a transcript.
pub(crate) fn is_transcript(relative_to_sessions: &Path) -> bool {
    let parts: Vec<Component<'_>> = relative_to_sessions.components().collect();
    matches!(
        parts.as_slice(),
        [Component::Normal(session), Component::Normal(file)]
            if *file == EVENTS && !session.to_string_lossy().starts_with('.')
    )
}

/// What the session whose transcript is `events` says about itself: its working folder, title
/// and client from `workspace.yaml` beside it, and the version of Copilot from its first event.
#[must_use]
pub fn session(events: &Path) -> SessionDetails {
    let mut session = SessionDetails::default();
    if let Some(workspace) = events
        .parent()
        .and_then(|folder| read_capped(&folder.join(WORKSPACE)))
        .and_then(|text| YamlLoader::load_from_str(&text).ok())
        .and_then(|documents| documents.into_iter().next())
    {
        let text = |key: &str| match &workspace[key] {
            Yaml::String(text) if !text.trim().is_empty() => Some(text.clone()),
            _ => None,
        };
        session.cwd = text("cwd");
        session.title = text("name");
        session.user_named = workspace["user_named"].as_bool().unwrap_or(false);
        session.client = text("client_name");
    }
    if let Some(start) = first_event(events) {
        session.version = start.version;
        if session.cwd.is_none() {
            session.cwd = start.cwd;
        }
    }
    session
}

/// The details a session's first event, `session.start`, records.
struct Start {
    version: Option<String>,
    cwd: Option<String>,
}

fn first_event(events: &Path) -> Option<Start> {
    #[derive(Deserialize)]
    struct RawStart {
        #[serde(rename = "type", default, deserialize_with = "lenient")]
        kind: Option<String>,
        #[serde(default, deserialize_with = "object")]
        data: Option<RawStartData>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RawStartData {
        #[serde(default, deserialize_with = "lenient")]
        copilot_version: Option<String>,
        #[serde(default, deserialize_with = "object")]
        context: Option<RawContext>,
    }
    #[derive(Deserialize)]
    struct RawContext {
        #[serde(default, deserialize_with = "lenient")]
        cwd: Option<String>,
    }

    let mut line = Vec::new();
    BufReader::new(open_shared(events).ok()?.take(MAX_DETAILS))
        .read_until(b'\n', &mut line)
        .ok()?;
    let start: RawStart = serde_json::from_slice(line.trim_ascii()).ok()?;
    if start.kind.as_deref() != Some("session.start") {
        return None;
    }
    let data = start.data?;
    Some(Start {
        version: data.copilot_version,
        cwd: data.context.and_then(|context| context.cwd),
    })
}

fn read_capped(path: &Path) -> Option<String> {
    let mut text = String::new();
    open_shared(path)
        .ok()?
        .take(MAX_DETAILS)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// The kinds of event the history uses. A line that names none of them is skipped unread, which
/// matters because the events that hold whole conversations are by far the largest.
const KEPT: [&[u8]; 4] = [
    b"\"session.usage_record\"",
    b"\"model.model_call_success\"",
    b"\"subagent.started\"",
    b"\"subagent.completed\"",
];

/// Reads one line of a Copilot CLI transcript, with or without its line ending.
///
/// Returns `None` for an event the history does not use.
///
/// # Errors
///
/// [`Unreadable`] for a line that is not JSON, or a request that lacks what identifies it.
pub fn parse_line(line: &[u8]) -> Result<Option<Record>, Unreadable> {
    let line = line.trim_ascii();
    if line.is_empty() {
        return Ok(None);
    }
    if line.first() != Some(&b'{') {
        return Err(Unreadable::NotJson);
    }
    if !KEPT.iter().any(|kind| memmem::find(line, kind).is_some()) {
        return Ok(None);
    }
    let event: RawEvent = serde_json::from_slice(line).map_err(|_| Unreadable::NotJson)?;
    match event.kind.as_deref() {
        Some("session.usage_record") => {
            request(event).map(|request| Some(Record::Request(Box::new(request))))
        }
        Some("model.model_call_success") => Ok(allowance(event).map(Record::Limit)),
        Some("subagent.started" | "subagent.completed") => {
            let data = event.data.unwrap_or_default();
            Ok(data
                .tool_call_id
                .zip(data.agent_name)
                .map(|(subagent_id, agent_type)| Record::SubagentType {
                    subagent_id,
                    agent_type,
                }))
        }
        _ => Ok(None),
    }
}

fn request(event: RawEvent) -> Result<RequestRecord, Unreadable> {
    let timestamp_ms = timestamp_ms(event.timestamp.as_deref()).ok_or(Unreadable::Incomplete)?;
    let usage = event
        .data
        .and_then(|data| data.usage)
        .ok_or(Unreadable::Incomplete)?;
    let model = usage.model.clone().ok_or(Unreadable::Incomplete)?;
    let accounting = usage.accounting.unwrap_or_default();
    let id = accounting
        .usage_id
        .or(usage.api_call_id.clone())
        .or(event.id)
        .ok_or(Unreadable::Incomplete)?;
    let cache_read = usage.cache_read_tokens.unwrap_or(0);
    let cache_write = usage.cache_write_tokens.unwrap_or(0);
    // Copilot CLI counts cached input in its input tokens; the history keeps input apart.
    let input = usage
        .input_tokens
        .unwrap_or(0)
        .saturating_sub(cache_read.saturating_add(cache_write));
    let subagent_id = (usage.initiator.as_deref() == Some("sub-agent"))
        .then(|| {
            usage
                .parent_tool_call_id
                .clone()
                .or(accounting.source_session_id)
        })
        .flatten();
    let charge = if usage.is_byok == Some(true) {
        None
    } else {
        usage.copilot_usage.as_ref().and_then(charge)
    };
    Ok(RequestRecord {
        record_id: format!("copilot:{id}"),
        message_id: None,
        request_id: None,
        timestamp_ms,
        cwd: None,
        entrypoint: None,
        agent_version: None,
        speed: None,
        service_tier: None,
        inference_geo: None,
        skill: None,
        mcp_server: None,
        mcp_tool: None,
        web_search_requests: 0,
        web_fetch_requests: 0,
        usage: vec![ModelUsage {
            model,
            tokens: Tokens {
                input,
                output: usage.output_tokens.unwrap_or(0),
                cache_read,
                cache_write_5m: cache_write,
                cache_write_1h: 0,
            },
        }],
        reasoning_tokens: usage.reasoning_tokens.unwrap_or(0),
        charge,
        initiator: usage.initiator,
        subagent_id,
        model_name: usage.model_display_name,
    })
}

/// The charge a request recorded, by kind of token, adding up to the total Copilot CLI recorded.
/// A kind of token this version does not know counts with the input, so that the charge stays
/// whole.
fn charge(usage: &RawCopilotUsage) -> Option<Charge> {
    if usage.token_details.is_empty() {
        return None;
    }
    let mut charge = Charge::default();
    for detail in &usage.token_details {
        let (Some(count), Some(per_batch), Some(batch)) =
            (detail.token_count, detail.cost_per_batch, detail.batch_size)
        else {
            return None;
        };
        if batch == 0 {
            return None;
        }
        let nano =
            u64::try_from(u128::from(count) * u128::from(per_batch) / u128::from(batch)).ok()?;
        let kind = match detail.token_type.as_deref() {
            Some("output") => &mut charge.output,
            Some("cache_read") => &mut charge.cache_read,
            Some("cache_write") => &mut charge.cache_write,
            _ => &mut charge.input,
        };
        *kind = kind.checked_add(nano)?;
    }
    if let Some(total) = usage.total_nano_aiu {
        let difference = i128::from(total) - i128::from(charge.total());
        charge.input = u64::try_from(i128::from(charge.input) + difference).ok()?;
    }
    Some(charge)
}

/// The monthly allowance a model call reports, as GitHub counted it at that moment; `None` for
/// an unlimited one, which has no limit to show.
fn allowance(event: RawEvent) -> Option<LimitSnapshot> {
    let observed_ms = timestamp_ms(event.timestamp.as_deref())?;
    let quota = event.data?.quota_snapshots?.premium_interactions?;
    if quota.is_unlimited_entitlement == Some(true) {
        return None;
    }
    let remaining = quota.remaining_percentage?;
    Some(LimitSnapshot {
        window: MONTHLY.to_owned(),
        source: Source::CopilotSessionFiles,
        observed_ms,
        used_percentage: 100.0 - remaining,
        resets_at_ms: timestamp_ms(quota.reset_date.as_deref()),
        used: quota.used_requests,
        entitlement: quota.entitlement_requests,
    })
}

fn timestamp_ms(text: Option<&str>) -> Option<i64> {
    Some(text?.parse::<jiff::Timestamp>().ok()?.as_millisecond())
}

#[derive(Deserialize)]
struct RawEvent {
    #[serde(rename = "type", default, deserialize_with = "lenient")]
    kind: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    timestamp: Option<String>,
    #[serde(default, deserialize_with = "object")]
    data: Option<RawData>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawData {
    #[serde(default, deserialize_with = "object")]
    usage: Option<RawUsage>,
    #[serde(default, deserialize_with = "object")]
    quota_snapshots: Option<RawQuotas>,
    #[serde(default, deserialize_with = "lenient")]
    tool_call_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    agent_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUsage {
    #[serde(default, deserialize_with = "lenient")]
    model: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    model_display_name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    input_tokens: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    output_tokens: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    cache_read_tokens: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    cache_write_tokens: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    reasoning_tokens: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    initiator: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    is_byok: Option<bool>,
    #[serde(default, deserialize_with = "lenient")]
    parent_tool_call_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    api_call_id: Option<String>,
    #[serde(default, deserialize_with = "object")]
    accounting: Option<RawAccounting>,
    #[serde(default, deserialize_with = "object")]
    copilot_usage: Option<RawCopilotUsage>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawAccounting {
    #[serde(default, deserialize_with = "lenient")]
    source_session_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    usage_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCopilotUsage {
    #[serde(default, deserialize_with = "lenient_list")]
    token_details: Vec<RawTokenDetail>,
    #[serde(default, deserialize_with = "lenient")]
    total_nano_aiu: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTokenDetail {
    #[serde(default, deserialize_with = "lenient")]
    batch_size: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    cost_per_batch: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    token_count: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    token_type: Option<String>,
}

#[derive(Deserialize)]
struct RawQuotas {
    #[serde(default, deserialize_with = "object")]
    premium_interactions: Option<RawQuota>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawQuota {
    #[serde(default, deserialize_with = "lenient")]
    entitlement_requests: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    used_requests: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    remaining_percentage: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    reset_date: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    is_unlimited_entitlement: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage_record(usage: &str) -> String {
        format!(
            r#"{{"type":"session.usage_record","id":"e1","timestamp":"2026-11-06T23:06:52.000Z","data":{{"usage":{usage}}}}}"#
        )
    }

    #[test]
    fn a_request_keeps_uncached_input_and_its_charge_by_kind() {
        let line = usage_record(
            r#"{"model":"claude-opus-5.5","modelDisplayName":"Claude Opus 5.5","inputTokens":30282,"outputTokens":49,"cacheReadTokens":0,"cacheWriteTokens":30278,"reasoningTokens":0,"cost":15.0,"initiator":"user","isByok":false,"accounting":{"sourceSessionId":"s1","sequence":1,"usageId":"msg_1"},"copilotUsage":{"tokenDetails":[{"batchSize":1000000,"costPerBatch":440000000000,"tokenCount":4,"tokenType":"input"},{"batchSize":1000000,"costPerBatch":22000000000,"tokenCount":0,"tokenType":"cache_read"},{"batchSize":1000000,"costPerBatch":550000000000,"tokenCount":30278,"tokenType":"cache_write"},{"batchSize":1000000,"costPerBatch":2200000000000,"tokenCount":49,"tokenType":"output"}],"totalNanoAiu":16762460000}}"#,
        );
        let Some(Record::Request(request)) = parse_line(line.as_bytes()).unwrap() else {
            panic!("a request");
        };
        assert_eq!(request.record_id, "copilot:msg_1");
        assert_eq!(request.usage[0].model, "claude-opus-5.5");
        assert_eq!(
            request.usage[0].tokens,
            Tokens {
                input: 4,
                output: 49,
                cache_read: 0,
                cache_write_5m: 30278,
                cache_write_1h: 0,
            }
        );
        let charge = request.charge.unwrap();
        assert_eq!(charge.total(), 16_762_460_000);
        assert_eq!(charge.cache_write, 16_652_900_000);
        assert_eq!(request.initiator.as_deref(), Some("user"));
        assert_eq!(request.model_name.as_deref(), Some("Claude Opus 5.5"));
        assert_eq!(request.subagent_id, None);
    }

    #[test]
    fn a_sub_agents_request_belongs_to_the_tool_call_that_started_it() {
        let line = usage_record(
            r#"{"model":"gpt-6-luna","inputTokens":5078,"outputTokens":88,"cacheReadTokens":0,"cacheWriteTokens":5075,"initiator":"sub-agent","parentToolCallId":"toolu_1","accounting":{"sourceSessionId":"s2","usageId":"msg_2"}}"#,
        );
        let Some(Record::Request(request)) = parse_line(line.as_bytes()).unwrap() else {
            panic!("a request");
        };
        assert_eq!(request.subagent_id.as_deref(), Some("toolu_1"));
        assert_eq!(request.charge, None, "no charge recorded");
    }

    #[test]
    fn a_request_on_your_own_key_has_no_charge() {
        let line = usage_record(
            r#"{"model":"gpt-6-luna","inputTokens":10,"outputTokens":1,"isByok":true,"accounting":{"usageId":"msg_3"},"copilotUsage":{"tokenDetails":[{"batchSize":1000000,"costPerBatch":100000000,"tokenCount":10,"tokenType":"input"}],"totalNanoAiu":1000}}"#,
        );
        let Some(Record::Request(request)) = parse_line(line.as_bytes()).unwrap() else {
            panic!("a request");
        };
        assert_eq!(request.charge, None);
    }

    #[test]
    fn the_allowance_is_read_from_a_model_call() {
        let line = r#"{"type":"model.model_call_success","timestamp":"2026-11-06T23:15:05.000Z","data":{"quotaSnapshots":{"chat":{"isUnlimitedEntitlement":true},"premium_interactions":{"isUnlimitedEntitlement":false,"entitlementRequests":45000,"usedRequests":1125,"remainingPercentage":97.5,"resetDate":"2026-12-01T00:00:00Z","overage":0}}}}"#;
        let Some(Record::Limit(reading)) = parse_line(line.as_bytes()).unwrap() else {
            panic!("a reading");
        };
        assert_eq!(reading.window, MONTHLY);
        assert_eq!(reading.source, Source::CopilotSessionFiles);
        assert!((reading.used_percentage - 2.5).abs() < 1e-9);
        assert_eq!(reading.used, Some(1125.0));
        assert_eq!(reading.entitlement, Some(45000.0));
        assert_eq!(reading.resets_at_ms, Some(1_796_083_200_000));
    }

    #[test]
    fn an_unlimited_allowance_is_no_limit() {
        let line = r#"{"type":"model.model_call_success","timestamp":"2026-11-06T23:15:05.000Z","data":{"quotaSnapshots":{"premium_interactions":{"isUnlimitedEntitlement":true,"remainingPercentage":100}}}}"#;
        assert_eq!(parse_line(line.as_bytes()), Ok(None));
    }

    #[test]
    fn a_sub_agent_is_typed_by_its_tool_call() {
        let line = r#"{"type":"subagent.started","timestamp":"2026-11-06T23:18:12.000Z","data":{"agentName":"explore","agentType":"explore","toolCallId":"toolu_1","model":"gpt-6-luna"}}"#;
        assert_eq!(
            parse_line(line.as_bytes()),
            Ok(Some(Record::SubagentType {
                subagent_id: "toolu_1".to_owned(),
                agent_type: "explore".to_owned()
            }))
        );
    }

    #[test]
    fn other_events_are_skipped_and_broken_lines_counted() {
        let snapshot = r#"{"type":"model.messages_snapshot","data":{"messages":[{"role":"user","content":"hello"}]}}"#;
        assert_eq!(parse_line(snapshot.as_bytes()), Ok(None));
        assert_eq!(parse_line(b""), Ok(None));
        assert_eq!(parse_line(b"not json"), Err(Unreadable::NotJson));
        assert_eq!(
            parse_line(br#"{"type":"session.usage_record","data":{"usage":{}}}"#),
            Err(Unreadable::Incomplete)
        );
    }

    #[test]
    fn only_a_sessions_events_are_its_transcript() {
        assert!(is_transcript(Path::new("981f/events.jsonl")));
        assert!(!is_transcript(Path::new("981f/workspace.yaml")));
        assert!(!is_transcript(Path::new(
            ".session-operation-locks/events.jsonl"
        )));
        assert!(!is_transcript(Path::new("events.jsonl")));
    }

    #[test]
    fn a_session_is_described_by_its_workspace_and_first_event() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join(SESSIONS).join("981f");
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join(WORKSPACE),
            "id: 981f\ncwd: C:\\Users\\ada\\code\\app\nclient_name: github/cli\nname: >-\n  Fix the importer so that it reads every\n  transcript\nuser_named: true\n",
        )
        .unwrap();
        fs::write(
            folder.join(EVENTS),
            "{\"type\":\"session.start\",\"data\":{\"copilotVersion\":\"1.0.106\",\"context\":{\"cwd\":\"C:\\\\elsewhere\"}}}\n",
        )
        .unwrap();

        let found = find(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].session_id, "981f");
        assert_eq!(found[0].agent, Agent::CopilotCli);
        let details = session(&found[0].path);
        assert_eq!(
            details,
            SessionDetails {
                cwd: Some(r"C:\Users\ada\code\app".to_owned()),
                title: Some("Fix the importer so that it reads every transcript".to_owned()),
                user_named: true,
                client: Some("github/cli".to_owned()),
                version: Some("1.0.106".to_owned()),
            }
        );
        assert!(!started_by_vscode(&details));
    }

    #[test]
    fn a_session_that_vs_code_started_names_its_client() {
        let client = |name: &str| SessionDetails {
            client: Some(name.to_owned()),
            ..SessionDetails::default()
        };
        assert!(started_by_vscode(&client("vscode-agent-host")));
        assert!(started_by_vscode(&client("vscode")));
        assert!(!started_by_vscode(&client("github/cli")));
        assert!(!started_by_vscode(&client("github/autopilot")));
        assert!(!started_by_vscode(&SessionDetails::default()));
    }
}
