//! Ingest: reading what the agents' transcripts added since the last ingest into the history.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::num::NonZero;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::agent::{Agent, VscodeHarness};
use crate::history::{
    Attribution, History, HistoryError, Identity, ProjectKey, ReadPosition, ReportedCost, Request,
    SessionKey, SessionTitle, Writer,
};
use crate::roots::DataRoot;
use crate::transcript::{
    self, ModelUsage, Parser, Read, Record, RequestRecord, SessionDetails, Transcript,
};
use crate::{copilot, stats_cache, vscode};

/// How much of a transcript one read takes in, so that a large transcript is written in parts.
const CHUNK_BYTES: u64 = 8 << 20;

/// How many threads read transcripts at most, while the calling thread writes what they read.
/// More readers than this only wait for the writer.
const MAX_READERS: usize = 4;

/// How many records one transaction takes in before it commits, unless a single read holds more,
/// so that an ingest never keeps other processes from writing for long.
const RECORDS_PER_WRITE: usize = 4096;

/// What one ingest did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Transcripts with content added since the last ingest.
    pub transcripts_read: u64,
    /// Transcripts that had not changed.
    pub transcripts_unchanged: u64,
    /// Request records read; a streamed response leaves several for one request, and a VS Code
    /// chat session file, read whole whenever it changes, its every prompt again.
    pub request_records: u64,
    /// Requests that the history gained or that changed in it.
    pub requests_changed: u64,
    /// When the earliest and the latest of the requests that changed were made.
    pub changed_between: Option<(i64, i64)>,
    /// Lines that this version cannot read.
    pub unreadable_lines: u64,
    /// Days with tokens from Claude Code's statistics, summed over the data roots.
    pub daily_totals: u64,
    /// Files that could not be read, such as a transcript Claude Code deleted meanwhile, with the
    /// reason.
    pub failed: Vec<(PathBuf, String)>,
    /// When the earliest and the latest request records read were written.
    pub requests_between: Option<(i64, i64)>,
    /// The ingest stopped before the end, as its progress callback asked; the next ingest carries
    /// on from where it stopped.
    pub stopped: bool,
}

/// Reads what the transcripts in `roots` added since the last ingest into the history, starting
/// with the most recently changed transcripts so that recent days are complete first, and then
/// the daily totals from Claude Code's statistics.
///
/// Returns `None` without reading anything while another process holds the ingest lock.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be written. A transcript that cannot be read is
/// listed in the report instead.
pub fn ingest(history: &mut History, roots: &[DataRoot]) -> Result<Option<Report>, HistoryError> {
    ingest_with_progress(history, roots, |_| ControlFlow::Continue(()))
}

/// How far an ingest has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// Transcripts looked at so far, read or found unchanged.
    pub done: usize,
    pub total: usize,
    /// Request records written so far.
    pub request_records: u64,
    /// When the earliest and the latest request records written so far were written.
    pub requests_between: Option<(i64, i64)>,
    /// Requests that the history gained or that changed in it so far.
    pub requests_changed: u64,
    /// When the earliest and the latest of the requests that changed so far were made.
    pub changed_between: Option<(i64, i64)>,
}

/// [`ingest`], calling `progress` once it knows which transcripts changed, and again after each
/// part of a transcript it writes.
///
/// When `progress` breaks, the ingest stops there: what it wrote stays in the history, the report
/// says it stopped, and the next ingest carries on from there.
///
/// # Errors
///
/// As for [`ingest`].
pub fn ingest_with_progress(
    history: &mut History,
    roots: &[DataRoot],
    progress: impl FnMut(Progress) -> ControlFlow<()>,
) -> Result<Option<Report>, HistoryError> {
    let Some(_lock) = history.try_lock_ingest()? else {
        return Ok(None);
    };
    let mut transcripts: Vec<Transcript> = roots
        .iter()
        .flat_map(|root| match root.agent {
            Agent::ClaudeCode => transcript::find(&root.path),
            Agent::CopilotCli => copilot::find(&root.path),
            Agent::CopilotVscode => vscode::find(&root.path),
        })
        .collect();
    transcripts.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.path.cmp(&b.path))
    });
    let positions = history.read_positions()?;
    let mut report = read_transcripts(history, &transcripts, &positions, progress)?;
    if report.stopped {
        return Ok(Some(report));
    }
    let gone = gone(roots, &transcripts, &positions);
    if !gone.is_empty() {
        history.write(|w| {
            for path in &gone {
                w.forget_transcript(path)?;
            }
            Ok(())
        })?;
    }

    let mut totals: BTreeMap<(String, String), u64> = BTreeMap::new();
    for root in roots.iter().filter(|root| root.agent == Agent::ClaudeCode) {
        let root = &root.path;
        match stats_cache::daily_totals(root) {
            Ok(days) => {
                for day in days {
                    let total = totals.entry((day.day, day.model)).or_default();
                    *total = total.saturating_add(day.tokens);
                }
            }
            Err(error) => report
                .failed
                .push((root.join("stats-cache.json"), error.to_string())),
        }
    }
    if !totals.is_empty() {
        history.write(|w| {
            for ((day, model), tokens) in &totals {
                w.daily_total(day, model, *tokens)?;
            }
            Ok(())
        })?;
        report.daily_totals = totals
            .keys()
            .map(|(day, _)| day)
            .collect::<BTreeSet<_>>()
            .len() as u64;
    }
    Ok(Some(report))
}

/// Reads what the given transcripts added since the last ingest, such as one session's, without
/// looking for others or reading Claude Code's statistics.
///
/// Returns `None` without reading anything while another process holds the ingest lock.
///
/// # Errors
///
/// As for [`ingest`].
pub fn ingest_transcripts(
    history: &mut History,
    transcripts: &[Transcript],
) -> Result<Option<Report>, HistoryError> {
    let Some(_lock) = history.try_lock_ingest()? else {
        return Ok(None);
    };
    let paths: Vec<String> = transcripts
        .iter()
        .map(|transcript| path_key(&transcript.path))
        .collect();
    let positions = history.read_positions_of(&paths)?;
    read_transcripts(history, transcripts, &positions, |_| {
        ControlFlow::Continue(())
    })
    .map(Some)
}

/// The transcripts that the history has read but that are no longer there, such as those Claude
/// Code deleted after its clean-up period, in the data roots whose transcripts folder is there.
///
/// Forgetting how far they were read keeps the read positions from growing for ever. A data root
/// that cannot be reached, such as a WSL distribution that is not running, keeps its positions.
fn gone(
    roots: &[DataRoot],
    found: &[Transcript],
    positions: &HashMap<String, ReadPosition>,
) -> Vec<String> {
    let found: HashSet<String> = found
        .iter()
        .map(|transcript| path_key(&transcript.path))
        .collect();
    let folders: Vec<String> = roots
        .iter()
        .map(DataRoot::transcripts)
        .filter(|folder| folder.is_dir())
        .map(|folder| path_key(&folder.join("")))
        .collect();
    positions
        .keys()
        .filter(|path| !found.contains(*path))
        .filter(|path| {
            folders
                .iter()
                .any(|folder| path.starts_with(folder.as_str()))
        })
        .cloned()
        .collect()
}

/// A transcript with content added since the last ingest.
struct Changed<'a> {
    transcript: &'a Transcript,
    key: String,
    modified_ms: i64,
    /// Where reading starts.
    from: u64,
    /// The hash of its first line when it was last read.
    first_line: Option<i64>,
}

/// What a reading thread found in a transcript.
enum Found {
    /// The next part of transcript `index`; `last` once nothing more of it is there to read.
    Part {
        index: usize,
        read: Read,
        last: bool,
        /// The kind of agent a subagent is, with the first part of its transcript.
        agent_type: Option<String>,
        /// What a Copilot or VS Code session says about itself, with every part of its
        /// transcript.
        session: Option<SessionDetails>,
    },
    /// Transcript `index` could not be read, or not to the end.
    Failed { index: usize, error: String },
}

impl Found {
    fn records(&self) -> usize {
        match self {
            Self::Part { read, .. } => read.records.len(),
            Self::Failed { .. } => 0,
        }
    }
}

/// Reads the transcripts that changed since the last ingest and records what they added.
///
/// Several threads read and parse transcripts, taking them in the given order, while this thread
/// writes what they found. The parts of one transcript are written in order; transcripts finish
/// in roughly the order given, and the history does not depend on the order they finish in.
fn read_transcripts(
    history: &mut History,
    transcripts: &[Transcript],
    positions: &HashMap<String, ReadPosition>,
    mut progress: impl FnMut(Progress) -> ControlFlow<()>,
) -> Result<Report, HistoryError> {
    let mut report = Report::default();
    let mut changed = Vec::new();
    for transcript in transcripts {
        let key = path_key(&transcript.path);
        let modified_ms = milliseconds(transcript.modified);
        match positions.get(&key) {
            Some(known) if known.len == transcript.len && known.modified_ms == modified_ms => {
                report.transcripts_unchanged += 1;
            }
            known => changed.push(Changed {
                transcript,
                key,
                modified_ms,
                from: known.map_or(0, |known| known.read_to),
                first_line: known.and_then(|known| known.first_line),
            }),
        }
    }
    report.transcripts_read = changed.len() as u64;
    let total = transcripts.len();
    let mut done = total - changed.len();
    let first = Progress {
        done,
        total,
        request_records: 0,
        requests_between: None,
        requests_changed: 0,
        changed_between: None,
    };
    if progress(first).is_break() {
        report.stopped = true;
        return Ok(report);
    }

    let readers = thread::available_parallelism()
        .map_or(1, NonZero::get)
        .min(MAX_READERS)
        .min(changed.len());
    let next = AtomicUsize::new(0);
    let flow = thread::scope(|scope| {
        let (sender, receiver) = mpsc::sync_channel(2 * readers);
        let (changed, next) = (&changed, &next);
        for _ in 0..readers {
            let sender = sender.clone();
            scope.spawn(move || read_changed(changed, next, &sender));
        }
        drop(sender);
        // Returning drops the receiver before the scope waits for the readers, so a reader
        // blocked on a full channel stops once it has read its part.
        write_found(history, changed, &receiver, |found, changes| {
            match found {
                Found::Part { read, last, .. } => {
                    report.unreadable_lines += read.unreadable;
                    for record in &read.records {
                        if let Record::Request(request) = record {
                            report.request_records += 1;
                            report.requests_between =
                                Some(widen(report.requests_between, request.timestamp_ms));
                        }
                    }
                    report.requests_changed += changes.requests;
                    if let Some((earliest, latest)) = changes.between {
                        let (from, to) = widen(report.changed_between, earliest);
                        report.changed_between = Some((from, to.max(latest)));
                    }
                    if last {
                        done += 1;
                    }
                }
                Found::Failed { index, error } => {
                    report
                        .failed
                        .push((changed[index].transcript.path.clone(), error));
                    done += 1;
                }
            }
            progress(Progress {
                done,
                total,
                request_records: report.request_records,
                requests_between: report.requests_between,
                requests_changed: report.requests_changed,
                changed_between: report.changed_between,
            })
        })
    })?;
    report.stopped = flow.is_break();
    Ok(report)
}

/// Reads changed transcripts, taking the next one nobody has taken yet, until none is left or
/// nobody is writing any more.
fn read_changed(changed: &[Changed<'_>], next: &AtomicUsize, sender: &SyncSender<Found>) {
    loop {
        let index = next.fetch_add(1, Ordering::Relaxed);
        let Some(Changed {
            transcript,
            from,
            first_line,
            ..
        }) = changed.get(index)
        else {
            return;
        };
        let mut from = *from;
        let mut first_line = *first_line;
        let mut agent_type = transcript
            .subagent_id
            .as_ref()
            .and_then(|_| transcript::agent_type(&transcript.path));
        let (parse, session): (Parser, _) = match transcript.agent {
            Agent::ClaudeCode => (transcript::parse_line, None),
            Agent::CopilotCli => (
                copilot::parse_line,
                Some(copilot::session(&transcript.path)),
            ),
            // A VS Code chat session's log means something only from its first line, so it is
            // read whole, in one part, whenever it changes.
            Agent::CopilotVscode => {
                let found = match vscode::read(&transcript.path) {
                    Ok((read, session)) => Found::Part {
                        index,
                        read,
                        last: true,
                        agent_type: None,
                        session: Some(session),
                    },
                    Err(error) => Found::Failed {
                        index,
                        error: error.to_string(),
                    },
                };
                if sender.send(found).is_err() {
                    return;
                }
                continue;
            }
        };
        loop {
            let found =
                match transcript::read_from(&transcript.path, from, CHUNK_BYTES, parse, first_line)
                {
                    Ok(read) => {
                        let last = read.end == from || read.end >= transcript.len;
                        from = read.end;
                        first_line = read.first_line;
                        Found::Part {
                            index,
                            read,
                            last,
                            agent_type: agent_type.take(),
                            session: session.clone(),
                        }
                    }
                    Err(error) => Found::Failed {
                        index,
                        error: error.to_string(),
                    },
                };
            let more = matches!(found, Found::Part { last: false, .. });
            if sender.send(found).is_err() {
                return;
            }
            if !more {
                break;
            }
        }
    }
}

/// Writes what the readers found, a batch of parts per transaction, and hands each part on to
/// `then` once it is written, with what it changed; stops after a batch when `then` breaks for
/// one of its parts.
fn write_found(
    history: &mut History,
    changed: &[Changed<'_>],
    receiver: &Receiver<Found>,
    mut then: impl FnMut(Found, Changes) -> ControlFlow<()>,
) -> Result<ControlFlow<()>, HistoryError> {
    let mut batch = Vec::new();
    while let Ok(found) = receiver.recv() {
        let mut records = found.records();
        batch.push((found, Changes::default()));
        while records < RECORDS_PER_WRITE
            && let Ok(found) = receiver.try_recv()
        {
            records += found.records();
            batch.push((found, Changes::default()));
        }
        history.write(|w| {
            for (found, changes) in &mut batch {
                if let Found::Part {
                    index,
                    read,
                    last,
                    agent_type,
                    session,
                } = found
                {
                    let changed = &changed[*index];
                    // A transcript read only in part is recorded as no longer than that part, so
                    // that the next ingest reads the rest even if this one stops in between.
                    let position = ReadPosition {
                        read_to: read.end,
                        len: if *last {
                            changed.transcript.len
                        } else {
                            read.end
                        },
                        modified_ms: changed.modified_ms,
                        first_line: read.first_line,
                    };
                    let kept = Kept {
                        records: &read.records,
                        agent_type: agent_type.as_deref(),
                        session: session.as_ref(),
                    };
                    *changes = record(w, changed.transcript, &kept, &changed.key, position)?;
                }
            }
            Ok(())
        })?;
        let mut flow = ControlFlow::Continue(());
        for (found, changes) in batch.drain(..) {
            if then(found, changes).is_break() {
                flow = ControlFlow::Break(());
            }
        }
        if flow.is_break() {
            return Ok(flow);
        }
    }
    Ok(ControlFlow::Continue(()))
}

/// What one read of a transcript found that the history keeps.
struct Kept<'a> {
    records: &'a [Record],
    /// The kind of agent a subagent is.
    agent_type: Option<&'a str>,
    /// What a Copilot or VS Code session says about itself.
    session: Option<&'a SessionDetails>,
}

/// The requests one read of a transcript added to the history or changed in it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Changes {
    requests: u64,
    /// When the earliest and the latest of them were made.
    between: Option<(i64, i64)>,
}

impl Changes {
    fn add(&mut self, request: &RequestRecord, changed: bool) {
        if changed {
            self.requests += 1;
            self.between = Some(widen(self.between, request.timestamp_ms));
        }
    }
}

/// A span of times widened to take in `at`.
fn widen(span: Option<(i64, i64)>, at: i64) -> (i64, i64) {
    span.map_or((at, at), |(first, last)| (first.min(at), last.max(at)))
}

/// Records what one read of a transcript found, and where the next read starts, and says which
/// requests that changed.
fn record(
    w: &mut Writer<'_>,
    transcript: &Transcript,
    kept: &Kept<'_>,
    key: &str,
    position: ReadPosition,
) -> Result<Changes, HistoryError> {
    let changes = match transcript.agent {
        Agent::ClaudeCode => record_claude_code(w, transcript, kept)?,
        Agent::CopilotCli => record_copilot(w, transcript, kept)?,
        Agent::CopilotVscode => record_vscode(w, transcript, kept)?,
    };
    w.transcript_read(key, position)?;
    Ok(changes)
}

fn record_claude_code(
    w: &mut Writer<'_>,
    transcript: &Transcript,
    Kept {
        records,
        agent_type,
        ..
    }: &Kept<'_>,
) -> Result<Changes, HistoryError> {
    let mut changes = Changes::default();
    if records.is_empty() {
        return Ok(changes);
    }
    // A subagent may work in another folder, such as a worktree, so only the session's own
    // transcript names the project's working folder.
    let cwd = if transcript.subagent_id.is_none() {
        records.iter().find_map(|record| match record {
            Record::Request(request) => request.cwd.as_deref(),
            _ => None,
        })
    } else {
        None
    };
    let project = w.project(&transcript.project_folder, cwd)?;
    let session = w.session(&transcript.session_id, project, Agent::ClaudeCode, None)?;
    if let (Some(subagent_id), Some(agent_type)) = (&transcript.subagent_id, agent_type) {
        w.subagent_type(session, subagent_id, agent_type)?;
    }
    let context = Context {
        agent: Agent::ClaudeCode,
        subagent_id: transcript.subagent_id.as_deref(),
        entrypoint: None,
        agent_version: None,
        per_prompt: false,
    };
    for record in *records {
        match record {
            Record::Request(request) => {
                changes.add(request, record_request(w, session, &context, request)?);
            }
            Record::AiTitle(title) => w.session_title(session, SessionTitle::Ai(title))?,
            Record::CustomTitle(title) => {
                w.session_title(session, SessionTitle::Custom(title))?;
            }
            Record::CostState(state) => {
                for cost in &state.models {
                    w.reported_cost(
                        session,
                        state.started_ms,
                        &ReportedCost {
                            model: &cost.model,
                            cost_usd: cost.cost_usd,
                            input: cost.input,
                            output: cost.output,
                            cache_read: cost.cache_read,
                            cache_write: cost.cache_write,
                        },
                    )?;
                }
            }
            Record::Limit(_) | Record::SubagentType { .. } => {}
        }
    }
    Ok(changes)
}

/// Records a part of a Copilot session's transcript: Copilot CLI's, or Copilot in VS Code's when
/// VS Code started the session on its Copilot harness.
fn record_copilot(
    w: &mut Writer<'_>,
    transcript: &Transcript,
    Kept {
        records, session, ..
    }: &Kept<'_>,
) -> Result<Changes, HistoryError> {
    let details = session.cloned().unwrap_or_default();
    let (agent, harness) = if copilot::started_by_vscode(&details) {
        (Agent::CopilotVscode, Some(VscodeHarness::Copilot))
    } else {
        (Agent::CopilotCli, None)
    };
    let kind = SessionKind {
        agent,
        harness,
        per_prompt: false,
    };
    record_session(w, transcript, records, &details, kind)
}

/// Records a VS Code chat session file, read whole: a session of Copilot in VS Code's Local
/// harness, whose requests are prompts.
fn record_vscode(
    w: &mut Writer<'_>,
    transcript: &Transcript,
    Kept {
        records, session, ..
    }: &Kept<'_>,
) -> Result<Changes, HistoryError> {
    let details = session.cloned().unwrap_or_default();
    let kind = SessionKind {
        agent: Agent::CopilotVscode,
        harness: Some(VscodeHarness::Local),
        per_prompt: true,
    };
    record_session(w, transcript, records, &details, kind)
}

/// Whose a session is, where its transcript alone does not say.
#[derive(Clone, Copy)]
struct SessionKind {
    agent: Agent,
    harness: Option<VscodeHarness>,
    /// Whether its requests are prompts, as VS Code's chat session files record them.
    per_prompt: bool,
}

/// Records a part of a session whose agent keeps the session's details apart from its records.
/// Its project is the working folder the session names, shared with every agent's sessions in
/// the same folder; a session with nothing new to record only takes its latest title, once the
/// history holds it.
fn record_session(
    w: &mut Writer<'_>,
    transcript: &Transcript,
    records: &[Record],
    details: &SessionDetails,
    kind: SessionKind,
) -> Result<Changes, HistoryError> {
    let mut changes = Changes::default();
    let key = if records.is_empty() {
        match w.known_session(&transcript.session_id)? {
            Some(key) => key,
            None => return Ok(changes),
        }
    } else {
        let project = project_of(w, details.cwd.as_deref())?;
        w.session(&transcript.session_id, project, kind.agent, kind.harness)?
    };
    if let Some(title) = &details.title {
        let title = if details.user_named {
            SessionTitle::Custom(title)
        } else {
            SessionTitle::Ai(title)
        };
        w.session_title(key, title)?;
    }
    for record in records {
        match record {
            Record::Request(request) => {
                let context = Context {
                    agent: kind.agent,
                    subagent_id: request.subagent_id.as_deref(),
                    entrypoint: details.client.as_deref(),
                    agent_version: details.version.as_deref(),
                    per_prompt: kind.per_prompt,
                };
                changes.add(request, record_request(w, key, &context, request)?);
            }
            Record::Limit(snapshot) => w.limit_snapshot(snapshot)?,
            Record::SubagentType {
                subagent_id,
                agent_type,
            } => w.subagent_type(key, subagent_id, agent_type)?,
            Record::AiTitle(_) | Record::CustomTitle(_) | Record::CostState(_) => {}
        }
    }
    Ok(changes)
}

/// The project whose working folder is `cwd`: the one the history already knows by it, or one
/// known by the folder Claude Code would keep its transcripts in.
fn project_of(w: &mut Writer<'_>, cwd: Option<&str>) -> Result<ProjectKey, HistoryError> {
    match cwd {
        Some(cwd) => match w.project_with_path(cwd)? {
            Some(project) => Ok(project),
            None => w.project(&transcript::project_folder(cwd), Some(cwd)),
        },
        None => w.project(UNKNOWN_PROJECT, None),
    }
}

/// The project of a session that names no working folder.
const UNKNOWN_PROJECT: &str = "unknown";

/// What a request takes from where it was read, besides its record.
struct Context<'a> {
    agent: Agent,
    subagent_id: Option<&'a str>,
    entrypoint: Option<&'a str>,
    agent_version: Option<&'a str>,
    /// Whether the transcript records a prompt and every model call it led to as one request.
    per_prompt: bool,
}

/// Records the request a transcript record holds: one request per model it used. Says whether
/// that changed the history.
///
/// The record's own model keeps the request's identity. An attempt on another model, such as
/// one Claude Code made before falling back, is a request of its own, told apart by that model's
/// name added to its most specific id.
fn record_request(
    w: &mut Writer<'_>,
    session: SessionKey,
    context: &Context<'_>,
    request: &RequestRecord,
) -> Result<bool, HistoryError> {
    if let (Some(name), Some(usage)) = (&request.model_name, request.usage.first()) {
        w.model_name(context.agent, &usage.model, name)?;
    }
    let mut changed = false;
    for (index, ModelUsage { model, tokens }) in request.usage.iter().enumerate() {
        let tag = |id: &str| {
            if index == 0 {
                id.to_owned()
            } else {
                format!("{id}@{model}")
            }
        };
        let (message_id, request_id) = match (&request.message_id, &request.request_id) {
            (Some(message), Some(request)) => (Some(message.clone()), Some(tag(request))),
            (Some(message), None) => (Some(tag(message)), None),
            (None, request) => (None, request.clone()),
        };
        let record_id = tag(&request.record_id);
        let first = index == 0;
        changed |= w.request_changes(&Request {
            session,
            identity: Identity::from_ids(message_id.as_deref(), request_id.as_deref()),
            record_id: &record_id,
            subagent_id: context.subagent_id,
            timestamp_ms: request.timestamp_ms,
            model,
            tokens: *tokens,
            web_search_requests: if first {
                request.web_search_requests
            } else {
                0
            },
            web_fetch_requests: if first { request.web_fetch_requests } else { 0 },
            speed: request.speed.as_deref(),
            service_tier: request.service_tier.as_deref(),
            inference_geo: request.inference_geo.as_deref(),
            attribution: Attribution {
                skill: request.skill.as_deref(),
                mcp_server: request.mcp_server.as_deref(),
                mcp_tool: request.mcp_tool.as_deref(),
            },
            entrypoint: request.entrypoint.as_deref().or(context.entrypoint),
            agent_version: request.agent_version.as_deref().or(context.agent_version),
            imported_from: None,
            reasoning_tokens: if first { request.reasoning_tokens } else { 0 },
            initiator: request.initiator.as_deref(),
            charge: if first { request.charge } else { None },
            per_prompt: context.per_prompt,
        })?;
    }
    Ok(changed)
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn milliseconds(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |since| {
        i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
    })
}
