//! Transcripts: finding Claude Code's, and reading what any agent's transcript added since the
//! last read.
//!
//! The formats are internal to the agents and change between versions, so reading is tolerant:
//! unknown fields and record types are ignored, and a line that cannot be read is counted and
//! skipped.

mod record;

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read as _, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use walkdir::WalkDir;

use crate::agent::Agent;

pub use record::{
    CostState, ModelUsage, Record, ReportedModelCost, RequestRecord, Unreadable, parse_line,
};

/// A transcript in a data root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    pub path: PathBuf,
    pub agent: Agent,
    /// For Claude Code, the folder in the data root's projects folder that holds the project's
    /// transcripts; empty for Copilot CLI, whose sessions name their working folder instead.
    pub project_folder: String,
    /// The session the transcript belongs to.
    pub session_id: String,
    /// The subagent whose transcript this is; `None` for the session's own transcript.
    pub subagent_id: Option<String>,
    pub len: u64,
    /// When the transcript last changed; for Copilot CLI, also when its session's details did.
    pub modified: SystemTime,
}

/// Every transcript in `root`'s projects folder, most recently changed first.
///
/// A session's transcript is `projects/<project>/<session>.jsonl`, and each of its subagents has
/// an `agent-<id>.jsonl` anywhere under `projects/<project>/<session>/subagents`, including the
/// subagents that workflows start. Other files there, such as workflow journals, are not
/// transcripts.
#[must_use]
pub fn find(root: &Path) -> Vec<Transcript> {
    let projects = root.join("projects");
    let mut found: Vec<Transcript> = WalkDir::new(&projects)
        .min_depth(2)
        .into_iter()
        // Within a session's folder only `subagents` can hold transcripts (`classify`), so the
        // walk leaves the rest, such as the tool results Claude Code keeps there, unvisited.
        .filter_entry(|entry| entry.depth() != 3 || entry.file_name() == "subagents")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let relative = entry.path().strip_prefix(&projects).ok()?;
            let metadata = entry.metadata().ok()?;
            classify(relative).map(|(project_folder, session_id, subagent_id)| Transcript {
                path: entry.path().to_owned(),
                agent: Agent::ClaudeCode,
                project_folder,
                session_id,
                subagent_id,
                len: metadata.len(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            })
        })
        .collect();
    found.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.path.cmp(&b.path))
    });
    found
}

/// A session's transcripts: its own, at `session_transcript`, and its subagents'. Claude Code
/// passes the session's own transcript to a status line.
#[must_use]
pub fn for_session(session_transcript: &Path) -> Vec<Transcript> {
    let (Some(project_dir), Some(stem)) = (
        session_transcript.parent(),
        session_transcript
            .file_stem()
            .and_then(|stem| stem.to_str()),
    ) else {
        return Vec::new();
    };
    let Some(project_folder) = project_dir.file_name().and_then(|name| name.to_str()) else {
        return Vec::new();
    };
    let mut paths = vec![session_transcript.to_owned()];
    paths.extend(
        WalkDir::new(project_dir.join(stem).join("subagents"))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .map(walkdir::DirEntry::into_path),
    );
    paths
        .into_iter()
        .filter_map(|path| {
            let relative = Path::new(project_folder).join(path.strip_prefix(project_dir).ok()?);
            let (project_folder, session_id, subagent_id) = classify(&relative)?;
            let metadata = std::fs::metadata(&path).ok()?;
            Some(Transcript {
                path,
                agent: Agent::ClaudeCode,
                project_folder,
                session_id,
                subagent_id,
                len: metadata.len(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            })
        })
        .collect()
}

/// The project folder, session id and subagent id of a path relative to the projects folder, or
/// `None` when the file is not a transcript.
fn classify(relative: &Path) -> Option<(String, String, Option<String>)> {
    let parts: Vec<&str> = relative
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let (file, folders) = parts.split_last()?;
    let stem = file.strip_suffix(".jsonl")?;
    match folders {
        [project] => Some(((*project).to_owned(), stem.to_owned(), None)),
        [project, session, "subagents", ..] => {
            let subagent_id = stem.strip_prefix("agent-")?;
            Some((
                (*project).to_owned(),
                (*session).to_owned(),
                Some(subagent_id.to_owned()),
            ))
        }
        _ => None,
    }
}

/// The most of a subagent's metadata file that is read.
const MAX_METADATA: u64 = 64 * 1024;

/// The kind of agent a subagent is, such as `Explore`, from the metadata file Claude Code keeps
/// beside its transcript: `agent-<id>.meta.json` next to `agent-<id>.jsonl`. Only the agent type
/// is read; the file also describes the subagent's task, which the history never stores.
#[must_use]
pub fn agent_type(transcript: &Path) -> Option<String> {
    use std::io::Read as _;

    #[derive(serde::Deserialize)]
    struct Metadata {
        #[serde(rename = "agentType")]
        agent_type: Option<String>,
    }
    let metadata = transcript.with_extension("meta.json");
    let mut text = String::new();
    open_shared(&metadata)
        .ok()?
        .take(MAX_METADATA)
        .read_to_string(&mut text)
        .ok()?;
    let agent_type = serde_json::from_str::<Metadata>(&text).ok()?.agent_type?;
    let agent_type = agent_type.trim();
    (!agent_type.is_empty()).then(|| agent_type.to_owned())
}

/// What one read of a transcript found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Read {
    /// The records the history uses, in the order the transcript holds them.
    pub records: Vec<Record>,
    /// Lines that are not records this version can read.
    pub unreadable: u64,
    /// Where the next read starts: just after the last complete line read.
    pub end: u64,
    /// Whether the read started again from the beginning, because the transcript was written
    /// again from the start since the previous read.
    pub restarted: bool,
    /// A hash of the transcript's first line, once it holds a whole one, which the next read
    /// compares with.
    pub first_line: Option<i64>,
}

/// Reads one line of an agent's transcript, as [`parse_line`] does Claude Code's.
pub type Parser = fn(&[u8]) -> Result<Option<Record>, Unreadable>;

/// How much of a transcript's first line its hash covers.
const FIRST_LINE_BYTES: u64 = 4096;

/// Reads the complete lines that follow byte `offset`, stopping after the first line that
/// reaches `limit` bytes, with `parse` reading each line.
///
/// A last line without its newline is left for the next read, because the agent may still be
/// writing it. The read starts again from the beginning when the transcript was written again
/// from the start since the previous read: it is shorter than `offset`, `offset` no longer
/// follows a line end, or its first line no longer matches `first_line`, the previous read's.
///
/// # Errors
///
/// Errors opening or reading the file, such as when the agent has deleted it.
pub fn read_from(
    path: &Path,
    offset: u64,
    limit: u64,
    parse: Parser,
    first_line: Option<i64>,
) -> io::Result<Read> {
    let file = open_shared(path)?;
    let len = file.metadata()?.len();
    let mut reader = BufReader::with_capacity(1 << 20, file);
    let first = first_line_hash(&mut reader)?;
    let restarted = offset > 0
        && (len < offset
            || matches!((first_line, first), (Some(known), Some(now)) if known != now)
            || !follows_line_end(&mut reader, offset)?);
    let start = if restarted { 0 } else { offset };
    reader.seek(SeekFrom::Start(start))?;

    let mut read = Read {
        end: start,
        restarted,
        first_line: first,
        ..Read::default()
    };
    let mut line = Vec::new();
    while read.end - start < limit {
        line.clear();
        let bytes = reader.read_until(b'\n', &mut line)?;
        if bytes == 0 || line.last() != Some(&b'\n') {
            break;
        }
        read.end += bytes as u64;
        match parse(&line) {
            Ok(Some(record)) => read.records.push(record),
            Ok(None) => {}
            Err(_) => read.unreadable += 1,
        }
    }
    Ok(read)
}

/// A hash of the first line's first [`FIRST_LINE_BYTES`], or `None` while the transcript holds
/// neither a whole first line nor that many bytes of it.
fn first_line_hash(reader: &mut BufReader<File>) -> io::Result<Option<i64>> {
    reader.seek(SeekFrom::Start(0))?;
    let mut line = Vec::new();
    reader
        .by_ref()
        .take(FIRST_LINE_BYTES)
        .read_until(b'\n', &mut line)?;
    let whole = line.last() == Some(&b'\n') || line.len() as u64 == FIRST_LINE_BYTES;
    Ok(whole.then(|| fnv1a(&line)))
}

/// Whether the byte before `offset` ends a line.
fn follows_line_end(reader: &mut BufReader<File>, offset: u64) -> io::Result<bool> {
    reader.seek(SeekFrom::Start(offset - 1))?;
    let mut byte = [0];
    reader.read_exact(&mut byte)?;
    Ok(byte == *b"\n")
}

/// The 64-bit FNV-1a hash of `bytes`, which stays the same across versions of Rust and of this
/// program, unlike the standard library's hashers.
fn fnv1a(bytes: &[u8]) -> i64 {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    });
    i64::from_ne_bytes(hash.to_ne_bytes())
}

/// Opens a transcript so that its agent can keep writing to it and delete it meanwhile.
pub(crate) fn open_shared(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        const FILE_SHARE_WRITE: u32 = 0x2;
        const FILE_SHARE_DELETE: u32 = 0x4;
        options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
    }
    options.open(path)
}

/// The folder Claude Code keeps a working folder's transcripts in: the path with every character
/// other than an ASCII letter or digit replaced by `-`. A project is known by it, whichever agent
/// worked in the folder.
#[must_use]
pub fn project_folder(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Whether a file, given relative to a data root's projects folder, is a transcript.
pub(crate) fn is_transcript(relative_to_projects: &Path) -> bool {
    classify(relative_to_projects).is_some()
}

#[cfg(test)]
mod tests {
    use super::{classify, open_shared, project_folder};
    use std::fs;
    use std::path::Path;

    #[test]
    fn working_folders_map_to_claude_codes_transcript_folders() {
        assert_eq!(
            project_folder(r"C:\Users\ada\Documents\Development\ClaudeDashboard"),
            "C--Users-ada-Documents-Development-ClaudeDashboard"
        );
        assert_eq!(
            project_folder("/home/ada/code/my_app.v2"),
            "-home-ada-code-my-app-v2"
        );
    }

    #[test]
    fn claude_code_can_delete_a_transcript_while_it_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("4f1c.jsonl");
        fs::write(&path, "{}\n").unwrap();
        let reading = open_shared(&path).unwrap();

        fs::remove_file(&path).unwrap();

        drop(reading);
        assert!(!path.exists());
    }

    fn kind(path: &str) -> Option<(String, String, Option<String>)> {
        classify(Path::new(path))
    }

    #[test]
    fn sessions_and_subagents_are_transcripts() {
        assert_eq!(
            kind("C--code-app/4f1c.jsonl"),
            Some(("C--code-app".into(), "4f1c".into(), None))
        );
        assert_eq!(
            kind("C--code-app/4f1c/subagents/agent-a1.jsonl"),
            Some(("C--code-app".into(), "4f1c".into(), Some("a1".into())))
        );
        assert_eq!(
            kind("C--code-app/4f1c/subagents/workflows/wf_9/agent-b2.jsonl"),
            Some(("C--code-app".into(), "4f1c".into(), Some("b2".into())))
        );
    }

    #[test]
    fn other_files_are_not_transcripts() {
        assert_eq!(
            kind("C--code-app/4f1c/subagents/workflows/wf_9/journal.jsonl"),
            None
        );
        assert_eq!(kind("C--code-app/4f1c/subagents/agent-a1.meta.json"), None);
        assert_eq!(kind("C--code-app/4f1c/tool-results/out.jsonl"), None);
        assert_eq!(kind("C--code-app/notes.txt"), None);
        assert_eq!(kind("history.jsonl"), None);
    }
}
