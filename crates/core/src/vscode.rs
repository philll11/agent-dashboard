//! Copilot in VS Code's Local harness: finding the chat session files VS Code keeps, and replaying
//! what each records (ADR 0017).
//!
//! VS Code keeps each chat of its Chat view in `chatSessions/<session id>.jsonl`, in the storage
//! folder of the window's workspace under `User/workspaceStorage`, or for a window without a folder
//! in `User/globalStorage/emptyWindowChatSessions`. The file is a log of changes to the session:
//! its first line holds the whole session, and each later line sets, deletes, or cuts and appends
//! at a path in it, which VS Code applies in order; after a few hundred changes VS Code writes the
//! file again from a fresh first line. A session that VS Code saved before version 1.109, or with
//! `chat.useLogSessionStorage` off, is one `<session id>.json` holding the whole session, which VS
//! Code reads only where there is no `.jsonl`.
//!
//! VS Code records usage per prompt: the output tokens of all the model calls a prompt led to, and
//! what the prompt cost in AI credits, but not the calls' input or cache tokens. VS Code documents
//! neither the files nor their format, so reading is tolerant: only the fields the history uses
//! are kept, others are skipped unread, and a line that cannot be applied is counted and skipped.
//! Chats on VS Code's Copilot harness are kept with Copilot CLI's sessions instead, and are read
//! there ([`crate::copilot`]).

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, BufRead, BufReader, Read as _};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Deserialize;
use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

use crate::agent::Agent;
use crate::lenient::lenient;
use crate::pricing::Charge;
use crate::tokens::Tokens;
use crate::transcript::{
    ModelUsage, Read, Record, RequestRecord, SessionDetails, Transcript, open_shared,
};

/// The folder in VS Code's user data folder that holds the person's settings and storage, and
/// every chat session file.
pub const USER: &str = "User";

/// The folder in [`USER`] that holds a storage folder for each workspace.
const WORKSPACE_STORAGE: &str = "workspaceStorage";

/// The folder in [`USER`] that holds storage shared by every window.
const GLOBAL_STORAGE: &str = "globalStorage";

/// The folder in a workspace's storage folder that holds its chat sessions.
const CHAT_SESSIONS: &str = "chatSessions";

/// The folder in [`GLOBAL_STORAGE`] that holds the chat sessions of windows without a folder.
const EMPTY_WINDOW_SESSIONS: &str = "emptyWindowChatSessions";

/// The file in a workspace's storage folder that names the workspace.
const WORKSPACE_FILE: &str = "workspace.json";

/// A chat session's log.
const LOG: &str = "jsonl";

/// A chat session's whole state, where it has no log.
const FLAT: &str = "json";

/// The most of `workspace.json` that is read.
const MAX_WORKSPACE_FILE: u64 = 64 * 1024;

/// The vendor of the models Copilot serves, which a model id in VS Code starts with.
const COPILOT_VENDOR: &str = "copilot";

/// Every chat session file in `root`, VS Code's user data folder: one per session in each
/// workspace's storage folder and in the folder for windows without one, its log where it has
/// one.
#[must_use]
pub fn find(root: &Path) -> Vec<Transcript> {
    let user = root.join(USER);
    let mut folders: Vec<PathBuf> = fs::read_dir(user.join(WORKSPACE_STORAGE))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(CHAT_SESSIONS))
        .collect();
    folders.push(user.join(GLOBAL_STORAGE).join(EMPTY_WINDOW_SESSIONS));
    folders
        .iter()
        .flat_map(|folder| sessions_in(folder))
        .collect()
}

/// The chat session files in one folder: a session's log, or its whole state where it has none.
fn sessions_in(folder: &Path) -> Vec<Transcript> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: HashMap<String, (PathBuf, fs::Metadata)> = HashMap::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let (Some(session_id), Some(extension)) = (
            path.file_stem().and_then(|stem| stem.to_str()),
            path.extension().and_then(|extension| extension.to_str()),
        ) else {
            continue;
        };
        if extension != LOG && extension != FLAT {
            continue;
        }
        let Some(metadata) = entry.metadata().ok().filter(fs::Metadata::is_file) else {
            continue;
        };
        let has_log = files
            .get(session_id)
            .is_some_and(|(kept, _)| kept.extension().is_some_and(|kept| kept == LOG));
        if !has_log {
            files.insert(session_id.to_owned(), (path, metadata));
        }
    }
    files
        .into_iter()
        .map(|(session_id, (path, metadata))| Transcript {
            path,
            agent: Agent::CopilotVscode,
            project_folder: String::new(),
            session_id,
            subagent_id: None,
            len: metadata.len(),
            modified: metadata.modified().unwrap_or(UNIX_EPOCH),
        })
        .collect()
}

/// Reads a chat session file whole: replays its log, or takes its whole state, and returns a
/// request for each prompt that has finished, with what the session says about itself.
///
/// A last line without its newline is left out, because VS Code may still be writing it.
///
/// # Errors
///
/// Errors opening or reading the file, such as when VS Code has deleted it.
pub fn read(path: &Path) -> io::Result<(Read, SessionDetails)> {
    let file = open_shared(path)?;
    let len = file.metadata()?.len();
    let mut reader = BufReader::with_capacity(1 << 20, file);
    let mut replay = Replay::default();
    let mut read = Read::default();
    if path.extension().is_some_and(|extension| extension == FLAT) {
        let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
        match Pruned(&SESSION).deserialize(&mut deserializer) {
            Ok(state) => replay.state = state,
            Err(_) => read.unreadable += 1,
        }
        read.end = len;
    } else {
        let mut line = Vec::new();
        loop {
            line.clear();
            let bytes = reader.read_until(b'\n', &mut line)?;
            if bytes == 0 || line.last() != Some(&b'\n') {
                break;
            }
            read.end += bytes as u64;
            if replay.apply(&line).is_err() {
                read.unreadable += 1;
            }
        }
    }
    let details = SessionDetails {
        cwd: working_folder(path),
        title: replay.title(),
        ..SessionDetails::default()
    };
    read.records = replay
        .requests()
        .into_iter()
        .map(|request| Record::Request(Box::new(request)))
        .collect();
    Ok((read, details))
}

/// The fields of a chat session that the history uses, and the shape of what holds them; every
/// other field is skipped unread. A leaf is kept whole, whatever it holds.
enum Shape {
    Leaf,
    Object(&'static [(&'static str, Shape)]),
    List(&'static Shape),
}

static LEAF: Shape = Shape::Leaf;

static SESSION: Shape = Shape::Object(&[
    ("version", Shape::Leaf),
    ("customTitle", Shape::Leaf),
    ("computedTitle", Shape::Leaf),
    ("requests", Shape::List(&REQUEST)),
]);

static REQUEST: Shape = Shape::Object(&[
    ("requestId", Shape::Leaf),
    ("timestamp", Shape::Leaf),
    ("modelId", Shape::Leaf),
    ("modelState", Shape::Leaf),
    ("completionTokens", Shape::Leaf),
    ("copilotCredits", Shape::Leaf),
    ("agent", Shape::Object(&[("extensionVersion", Shape::Leaf)])),
    (
        "result",
        Shape::Object(&[
            ("details", Shape::Leaf),
            (
                "metadata",
                Shape::Object(&[("toolCallRounds", Shape::List(&ROUND))]),
            ),
        ]),
    ),
]);

/// One model call of a prompt, which names the model it used.
static ROUND: Shape = Shape::Object(&[("modelId", Shape::Leaf)]);

/// The shape of what sits at `path` in a session, or `None` when the history does not keep it.
fn shape_at(path: &[Key]) -> Option<&'static Shape> {
    let mut shape = &SESSION;
    for key in path {
        shape = match (shape, key) {
            (Shape::Leaf, _) => return Some(&LEAF),
            (Shape::Object(fields), Key::Name(name)) => fields
                .iter()
                .find(|(field, _)| field == name)
                .map(|(_, shape)| shape)?,
            (Shape::List(item), Key::Index(_)) => item,
            _ => return None,
        };
    }
    Some(shape)
}

/// Reads a value as [`Shape`] says, keeping only the fields it names.
#[derive(Clone, Copy)]
struct Pruned(&'static Shape);

impl<'de> DeserializeSeed<'de> for Pruned {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        match self.0 {
            Shape::Leaf => Value::deserialize(deserializer),
            shape => deserializer.deserialize_any(PrunedVisitor(shape)),
        }
    }
}

struct PrunedVisitor(&'static Shape);

impl<'de> Visitor<'de> for PrunedVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any value")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let Shape::Object(fields) = self.0 else {
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            return Ok(Value::Null);
        };
        let mut kept = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            match fields.iter().find(|(field, _)| *field == key) {
                Some((_, shape)) => {
                    let value = map.next_value_seed(Pruned(shape))?;
                    kept.insert(key, value);
                }
                None => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(Value::Object(kept))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Value, A::Error> {
        if let Shape::List(item) = self.0 {
            return Items(item).visit_seq(seq).map(Value::Array);
        }
        skip_seq(seq)?;
        Ok(Value::Null)
    }

    // Anything but an object or a list where one is expected keeps nothing.
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_str<E: de::Error>(self, _: &str) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
}

/// Reads a list whose items take one [`Shape`].
#[derive(Clone, Copy)]
struct Items(&'static Shape);

impl Items {
    fn visit_seq<'de, A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<Value>, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(Pruned(self.0))? {
            items.push(item);
        }
        Ok(items)
    }
}

impl<'de> DeserializeSeed<'de> for Items {
    type Value = Vec<Value>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Vec<Value>, D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for Items {
    type Value = Vec<Value>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a list")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Vec<Value>, A::Error> {
        Items::visit_seq(self, seq)
    }
}

fn skip_seq<'de, A: SeqAccess<'de>>(mut seq: A) -> Result<(), A::Error> {
    while seq.next_element::<IgnoredAny>()?.is_some() {}
    Ok(())
}

/// One step of a path into a session: a field's name or an item's index.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Key {
    Index(usize),
    Name(String),
}

/// One line of a session's log.
#[derive(Deserialize)]
struct Entry<'a> {
    /// 0 for the whole session, 1 to set a value, 2 to cut and append to a list, 3 to delete.
    kind: u8,
    #[serde(default)]
    k: Vec<Key>,
    #[serde(borrow, default)]
    v: Option<&'a RawValue>,
    /// Where a list is cut before the items are appended.
    #[serde(default)]
    i: Option<usize>,
}

/// A log line that cannot be applied: not a change VS Code writes, or one whose path is not
/// there.
#[derive(Debug)]
struct Unapplied;

impl From<serde_json::Error> for Unapplied {
    fn from(_: serde_json::Error) -> Self {
        Self
    }
}

/// A session as far as its log has been replayed, holding only the fields the history uses.
#[derive(Default)]
struct Replay {
    state: Value,
}

impl Replay {
    /// Applies one line of the log, as VS Code does; a change to a field the history does not
    /// keep is skipped without reading its value.
    fn apply(&mut self, line: &[u8]) -> Result<(), Unapplied> {
        let entry: Entry<'_> = serde_json::from_slice(line.trim_ascii())?;
        if entry.kind == 0 {
            self.state = parse(entry.v, &SESSION)?;
            return Ok(());
        }
        let Some(shape) = shape_at(&entry.k) else {
            return Ok(());
        };
        let (last, parents) = entry.k.split_last().ok_or(Unapplied)?;
        let parent = descend(&mut self.state, parents).ok_or(Unapplied)?;
        match entry.kind {
            1 => {
                let value = parse(entry.v, shape)?;
                set(parent, last, value)
            }
            2 => {
                let item = match shape {
                    Shape::List(item) => item,
                    Shape::Leaf => &LEAF,
                    Shape::Object(_) => return Err(Unapplied),
                };
                let items = match entry.v {
                    Some(raw) => Items(item)
                        .deserialize(&mut serde_json::Deserializer::from_str(raw.get()))?,
                    None => Vec::new(),
                };
                push(parent, last, items, entry.i)
            }
            3 => {
                delete(parent, last);
                Ok(())
            }
            _ => Err(Unapplied),
        }
    }

    /// The session's title: the one VS Code or the person gave it.
    fn title(&self) -> Option<String> {
        [&self.state["customTitle"], &self.state["computedTitle"]]
            .into_iter()
            .find_map(|title| {
                let title = title.as_str()?.trim();
                (!title.is_empty()).then(|| title.to_owned())
            })
    }

    /// A request for each finished prompt that used tokens or recorded a charge.
    fn requests(&self) -> Vec<RequestRecord> {
        self.state["requests"]
            .as_array()
            .map(|requests| requests.iter().filter_map(request).collect())
            .unwrap_or_default()
    }
}

/// A value of `shape` from a line's `v`; a missing value is `null`, which VS Code writes too.
fn parse(raw: Option<&RawValue>, shape: &'static Shape) -> Result<Value, Unapplied> {
    match raw {
        Some(raw) => {
            Ok(Pruned(shape).deserialize(&mut serde_json::Deserializer::from_str(raw.get()))?)
        }
        None => Ok(Value::Null),
    }
}

/// The value at `path` below `node`.
fn descend<'v>(mut node: &'v mut Value, path: &[Key]) -> Option<&'v mut Value> {
    for key in path {
        node = match (node, key) {
            (Value::Object(fields), Key::Name(name)) => fields.get_mut(name)?,
            (Value::Array(items), Key::Index(index)) => items.get_mut(*index)?,
            _ => return None,
        };
    }
    Some(node)
}

fn set(parent: &mut Value, key: &Key, value: Value) -> Result<(), Unapplied> {
    match (parent, key) {
        (Value::Object(fields), Key::Name(name)) => {
            fields.insert(name.clone(), value);
            Ok(())
        }
        (Value::Array(items), Key::Index(index)) if *index < items.len() => {
            items[*index] = value;
            Ok(())
        }
        (Value::Array(items), Key::Index(index)) if *index == items.len() => {
            items.push(value);
            Ok(())
        }
        _ => Err(Unapplied),
    }
}

/// Cuts the list at `key` to `cut` items, when given, then appends `items`; a list that is not
/// there yet starts empty.
fn push(
    parent: &mut Value,
    key: &Key,
    items: Vec<Value>,
    cut: Option<usize>,
) -> Result<(), Unapplied> {
    let list = match (parent, key) {
        (Value::Object(fields), Key::Name(name)) => {
            fields.entry(name.clone()).or_insert(Value::Null)
        }
        (Value::Array(list), Key::Index(index)) => list.get_mut(*index).ok_or(Unapplied)?,
        _ => return Err(Unapplied),
    };
    if !list.is_array() {
        *list = Value::Array(Vec::new());
    }
    let Value::Array(list) = list else {
        return Err(Unapplied);
    };
    if let Some(cut) = cut {
        list.truncate(cut);
    }
    list.extend(items);
    Ok(())
}

fn delete(parent: &mut Value, key: &Key) {
    match (parent, key) {
        (Value::Object(fields), Key::Name(name)) => {
            fields.remove(name);
        }
        (Value::Array(items), Key::Index(index)) => {
            if let Some(item) = items.get_mut(*index) {
                *item = Value::Null;
            }
        }
        _ => {}
    }
}

/// The request one prompt records, once it has finished and used tokens or recorded a charge.
///
/// A prompt is finished once VS Code says when it completed, was cancelled or failed; a prompt
/// saved by a VS Code too old to say is finished.
fn request(prompt: &Value) -> Option<RequestRecord> {
    let request_id = prompt["requestId"].as_str()?;
    let timestamp_ms = milliseconds(&prompt["timestamp"])?;
    let state = &prompt["modelState"];
    if !state.is_null() && !state["completedAt"].is_number() {
        return None;
    }
    let output = prompt["completionTokens"].as_u64().unwrap_or(0);
    let charge = prompt["copilotCredits"].as_f64().and_then(charge);
    if output == 0 && charge.is_none_or(|charge| charge.total() == 0) {
        return None;
    }
    let (model, alone) = model(prompt)?;
    Some(RequestRecord {
        record_id: format!("vscode:{request_id}"),
        message_id: None,
        request_id: None,
        timestamp_ms,
        cwd: None,
        entrypoint: None,
        agent_version: prompt["agent"]["extensionVersion"]
            .as_str()
            .map(str::to_owned),
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
                output,
                ..Tokens::default()
            },
        }],
        reasoning_tokens: 0,
        charge,
        initiator: None,
        subagent_id: None,
        model_name: alone.then(|| model_name(prompt)).flatten(),
    })
}

/// A time in milliseconds since the Unix epoch, which VS Code writes as a whole number.
#[allow(clippy::cast_possible_truncation)]
fn milliseconds(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| {
        let at = value.as_f64()?;
        at.is_finite().then_some(at as i64)
    })
}

/// A prompt's charge from its AI credits: one total, which VS Code does not split by kind of
/// token, held in the input part.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn charge(credits: f64) -> Option<Charge> {
    if !credits.is_finite() || credits < 0.0 {
        return None;
    }
    let nano = (credits * Charge::NANO_PER_CREDIT).round();
    let nano = (nano < u64::MAX as f64).then_some(nano as u64)?;
    Some(Charge {
        input: nano,
        ..Charge::default()
    })
}

/// The model a prompt used, by the ids Copilot CLI uses: the one most of its calls used, which
/// also resolves Auto, else the selected model without its vendor. A model of another vendor,
/// such as one on the person's own key, keeps its vendor. Says too whether it is the only model
/// the prompt's calls used.
fn model(prompt: &Value) -> Option<(String, bool)> {
    let selected = prompt["modelId"].as_str().filter(|id| !id.is_empty());
    if let Some((vendor, _)) = selected.and_then(|id| id.split_once('/'))
        && vendor != COPILOT_VENDOR
    {
        return selected.map(|id| (id.to_owned(), true));
    }
    let mut calls: Vec<(&str, usize)> = Vec::new();
    for round in prompt["result"]["metadata"]["toolCallRounds"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(model) = round["modelId"].as_str().filter(|id| !id.is_empty()) {
            match calls.iter_mut().find(|(seen, _)| *seen == model) {
                Some((_, count)) => *count += 1,
                None => calls.push((model, 1)),
            }
        }
    }
    let most = calls
        .iter()
        .enumerate()
        .max_by_key(|(order, (_, count))| (*count, std::cmp::Reverse(*order)))
        .map(|(_, (model, _))| ((*model).to_owned(), calls.len() == 1));
    most.or_else(|| {
        selected.map(|id| {
            let model = id.split_once('/').map_or(id, |(_, model)| model);
            (model.to_owned(), true)
        })
    })
}

/// The name VS Code shows for the model a prompt used, in the details under its response, such
/// as `Claude Sonnet 4.6` in `Claude Sonnet 4.6 • 1x`; what follows the model's name is the
/// prompt's cost in words, which the history never takes from there.
fn model_name(prompt: &Value) -> Option<String> {
    let details = prompt["result"]["details"].as_str()?;
    let end = [" \u{2022} ", " \u{b7} "]
        .iter()
        .filter_map(|separator| details.find(separator))
        .min()
        .unwrap_or(details.len());
    let name = details[..end].trim();
    (!name.is_empty()).then(|| name.to_owned())
}

/// The working folder of the session whose file is `path`: the folder, or the multi-root
/// workspace's folder, that `workspace.json` names in its workspace's storage folder; `None` for
/// a window without a folder.
fn working_folder(path: &Path) -> Option<String> {
    #[derive(Deserialize)]
    struct Workspace {
        #[serde(default, deserialize_with = "lenient")]
        folder: Option<String>,
        #[serde(default, deserialize_with = "lenient")]
        workspace: Option<String>,
    }

    let storage = path.parent()?.parent()?;
    if storage.parent()?.file_name()? != WORKSPACE_STORAGE {
        return None;
    }
    let mut text = String::new();
    open_shared(&storage.join(WORKSPACE_FILE))
        .ok()?
        .take(MAX_WORKSPACE_FILE)
        .read_to_string(&mut text)
        .ok()?;
    let workspace: Workspace = serde_json::from_str(&text).ok()?;
    if let Some(folder) = workspace.folder {
        return uri_path(&folder);
    }
    workspace_folder(&uri_path(&workspace.workspace?)?)
}

/// The folder of a multi-root workspace's file; `None` for an untitled workspace, whose file is a
/// `workspace.json` VS Code keeps for itself.
fn workspace_folder(file: &str) -> Option<String> {
    let cut = file.rfind(['/', '\\'])?;
    if file[cut + 1..].eq_ignore_ascii_case(WORKSPACE_FILE) {
        return None;
    }
    let folder = &file[..cut];
    Some(if folder.ends_with(':') {
        format!("{folder}\\")
    } else if folder.is_empty() {
        "/".to_owned()
    } else {
        folder.to_owned()
    })
}

/// The path a folder URI names, as the agents write working folders: a `file:` URI as a path of
/// the platform that wrote it, with an upper-case drive letter on Windows, and a `vscode-remote:`
/// URI, such as a WSL distribution's or an SSH host's, as the path on the remote machine.
fn uri_path(uri: &str) -> Option<String> {
    let (scheme, rest) = uri.split_once("://")?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = percent_decode(path)?;
    let path = path.trim_end_matches('/');
    match scheme {
        "file" if !authority.is_empty() => Some(format!(
            r"\\{}\{}",
            percent_decode(authority)?,
            path.replace('/', "\\")
        )),
        "file" => Some(local_path(path)),
        "vscode-remote" => Some(format!("/{path}")),
        _ => None,
    }
}

/// A local path from a `file:` URI's path, without its leading slash: `c:/Users/ada` on Windows,
/// `home/ada` elsewhere.
fn local_path(path: &str) -> String {
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let drive = char::from(bytes[0].to_ascii_uppercase());
        format!(
            "{drive}:\\{}",
            path[2..].trim_start_matches('/').replace('/', "\\")
        )
    } else {
        format!("/{path}")
    }
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = text.get(at + 1..at + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A finished prompt as VS Code saves it, with the heavy fields the history skips, which hold
    /// the word `unkept`.
    fn prompt(id: &str, tokens: u64, credits: f64) -> String {
        format!(
            r#"{{"requestId":"request_{id}","timestamp":1793000000000,"modelId":"copilot/claude-sonnet-4.6",
            "agent":{{"id":"github.copilot.editsAgent","extensionVersion":"0.73.0","metadata":{{}}}},
            "message":{{"text":"unkept","parts":[]}},"response":[{{"kind":"markdownContent","value":"unkept"}}],
            "modelState":{{"value":1,"completedAt":1793000060000}},"completionTokens":{tokens},"copilotCredits":{credits},
            "result":{{"metadata":{{"toolCallRounds":[{{"modelId":"claude-sonnet-4.6","toolCalls":[]}}],"toolCallResults":{{"x":{{"content":["unkept"]}}}}}}}}}}"#
        )
        .replace(['\n', ' '], "")
    }

    fn session_line(prompts: &[String]) -> String {
        format!(
            r#"{{"kind":0,"v":{{"version":3,"sessionId":"s1","creationDate":1793000000000,"customTitle":"Fix the importer","requests":[{}],"inputState":{{"inputText":"unkept"}}}}}}"#,
            prompts.join(",")
        )
    }

    fn replay(lines: &[String]) -> Replay {
        let mut replay = Replay::default();
        for line in lines {
            replay.apply(line.as_bytes()).unwrap();
        }
        replay
    }

    fn only(requests: &[RequestRecord]) -> &RequestRecord {
        assert_eq!(requests.len(), 1, "{requests:#?}");
        &requests[0]
    }

    #[test]
    fn a_finished_prompt_records_its_output_tokens_and_its_charge() {
        let replay = replay(&[session_line(&[prompt("1", 1690, 29.7)])]);
        let request = replay.requests();
        let request = only(&request);
        assert_eq!(request.record_id, "vscode:request_1");
        assert_eq!(request.timestamp_ms, 1_793_000_000_000);
        assert_eq!(request.agent_version.as_deref(), Some("0.73.0"));
        assert_eq!(
            request.usage,
            vec![ModelUsage {
                model: "claude-sonnet-4.6".to_owned(),
                tokens: Tokens {
                    output: 1690,
                    ..Tokens::default()
                },
            }]
        );
        let charge = request.charge.unwrap();
        assert_eq!(charge.total(), 29_700_000_000);
        assert_eq!(charge.input, charge.total(), "one total, in the input part");
        assert_eq!(replay.title().as_deref(), Some("Fix the importer"));
    }

    #[test]
    fn only_the_fields_the_history_uses_are_kept() {
        let replay = replay(&[session_line(&[prompt("1", 10, 1.0)])]);
        let text = replay.state.to_string();
        assert!(!text.contains("unkept"), "{text}");
        assert!(text.contains("toolCallRounds"));
    }

    #[test]
    fn changes_apply_as_vs_code_applies_them() {
        let replay = replay(&[
            session_line(&[prompt("1", 10, 1.0)]),
            // A second prompt arrives, still running.
            format!(
                r#"{{"kind":2,"k":["requests"],"v":[{}]}}"#,
                prompt("2", 0, 0.0)
                    .replace(r#""modelState":{"value":1,"completedAt":1793000060000}"#, r#""modelState":{"value":0}"#)
            ),
            // Its output grows, and the title changes.
            r#"{"kind":1,"k":["requests",1,"completionTokens"],"v":500}"#.to_owned(),
            r#"{"kind":1,"k":["customTitle"],"v":"Fix the importer's reads"}"#.to_owned(),
            // A change the history does not keep is skipped unread, even when its path is not
            // there.
            r#"{"kind":2,"k":["requests",7,"response"],"v":[{"kind":"markdownContent"}]}"#
                .to_owned(),
            // It finishes, with its charge.
            r#"{"kind":1,"k":["requests",1,"copilotCredits"],"v":4.2}"#.to_owned(),
            r#"{"kind":1,"k":["requests",1,"modelState"],"v":{"value":1,"completedAt":1793000120000}}"#
                .to_owned(),
        ]);
        let requests = replay.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].usage[0].tokens.output, 500);
        assert_eq!(requests[1].charge.unwrap().total(), 4_200_000_000);
        assert_eq!(replay.title().as_deref(), Some("Fix the importer's reads"));
    }

    #[test]
    fn a_running_prompt_waits_until_it_finishes() {
        let running = prompt("1", 300, 2.0).replace(
            r#""modelState":{"value":1,"completedAt":1793000060000}"#,
            r#""modelState":{"value":4}"#,
        );
        assert_eq!(replay(&[session_line(&[running])]).requests(), []);
        let cancelled = prompt("1", 300, 2.0)
            .replace(r#""modelState":{"value":1,"#, r#""modelState":{"value":2,"#);
        assert_eq!(replay(&[session_line(&[cancelled])]).requests().len(), 1);
    }

    #[test]
    fn a_cut_removes_prompts_and_a_delete_removes_a_field() {
        let replay = replay(&[
            session_line(&[
                prompt("1", 10, 1.0),
                prompt("2", 20, 2.0),
                prompt("3", 30, 3.0),
            ]),
            format!(
                r#"{{"kind":2,"k":["requests"],"v":[{}],"i":1}}"#,
                prompt("4", 40, 4.0)
            ),
            r#"{"kind":3,"k":["requests",1,"copilotCredits"]}"#.to_owned(),
            r#"{"kind":3,"k":["customTitle"]}"#.to_owned(),
        ]);
        let requests = replay.requests();
        let ids: Vec<&str> = requests.iter().map(|r| r.record_id.as_str()).collect();
        assert_eq!(ids, ["vscode:request_1", "vscode:request_4"]);
        assert_eq!(requests[1].charge, None);
        assert_eq!(replay.title(), None);
    }

    #[test]
    fn a_fresh_first_line_starts_the_session_again() {
        let replay = replay(&[
            session_line(&[prompt("1", 10, 1.0)]),
            session_line(&[prompt("1", 10, 1.0), prompt("2", 20, 2.0)]),
        ]);
        assert_eq!(replay.requests().len(), 2);
    }

    #[test]
    fn a_line_that_cannot_be_applied_is_refused() {
        let mut replay = Replay::default();
        replay.apply(session_line(&[]).as_bytes()).unwrap();
        assert!(replay.apply(b"not json").is_err());
        assert!(
            replay
                .apply(br#"{"kind":1,"k":["requests",5,"modelId"],"v":"x"}"#)
                .is_err()
        );
        assert!(replay.apply(br#"{"kind":9,"k":["customTitle"]}"#).is_err());
    }

    #[test]
    fn a_prompt_without_tokens_or_a_charge_is_left_out() {
        let requests = replay(&[session_line(&[
            prompt("1", 0, 0.0).replace(r#","copilotCredits":0"#, "")
        ])])
        .requests();
        assert_eq!(requests, []);
    }

    #[test]
    fn auto_takes_the_model_most_calls_used_and_another_vendor_keeps_its_own() {
        let auto = prompt("1", 10, 1.0)
            .replace("copilot/claude-sonnet-4.6", "copilot/auto")
            .replace(
                r#""toolCallRounds":[{"modelId":"claude-sonnet-4.6","toolCalls":[]}]"#,
                r#""toolCallRounds":[{"modelId":"gpt-6-luna"},{"modelId":"claude-sonnet-4.6"},{"modelId":"gpt-6-luna"}]"#,
            );
        let replay_auto = replay(&[session_line(&[auto])]);
        assert_eq!(only(&replay_auto.requests()).usage[0].model, "gpt-6-luna");

        let unresolved = prompt("1", 10, 1.0)
            .replace("copilot/claude-sonnet-4.6", "copilot/auto")
            .replace(r#"{"modelId":"claude-sonnet-4.6","toolCalls":[]}"#, "");
        let replay_unresolved = replay(&[session_line(&[unresolved])]);
        assert_eq!(only(&replay_unresolved.requests()).usage[0].model, "auto");

        let own_key = prompt("1", 10, 1.0)
            .replace("copilot/claude-sonnet-4.6", "anthropic/claude-sonnet-4-5")
            .replace(r#","copilotCredits":1"#, "");
        let replay_own = replay(&[session_line(&[own_key])]);
        let request = replay_own.requests();
        let request = only(&request);
        assert_eq!(request.usage[0].model, "anthropic/claude-sonnet-4-5");
        assert_eq!(request.charge, None);
    }

    #[test]
    fn a_prompt_on_one_model_takes_the_name_vs_code_shows_for_it() {
        let named = |details: &str, rounds: &str| {
            let text = prompt("1", 10, 1.0)
                .replace(
                    r#""result":{"metadata":"#,
                    &format!(r#""result":{{"details":"{details}","metadata":"#),
                )
                .replace(
                    r#""toolCallRounds":[{"modelId":"claude-sonnet-4.6","toolCalls":[]}]"#,
                    &format!(r#""toolCallRounds":{rounds}"#),
                );
            let replay = replay(&[session_line(&[text])]);
            replay.requests()[0].model_name.clone()
        };
        let one = r#"[{"modelId":"claude-sonnet-4.6"}]"#;
        assert_eq!(
            named("Claude Sonnet 4.6 \u{2022} 29.7 credits", one).as_deref(),
            Some("Claude Sonnet 4.6")
        );
        assert_eq!(
            named("Claude Sonnet 4.6 \u{b7} 1x", one).as_deref(),
            Some("Claude Sonnet 4.6")
        );
        let two = r#"[{"modelId":"claude-sonnet-4.6"},{"modelId":"gpt-6-luna"}]"#;
        assert_eq!(named("Claude Sonnet 4.6 \u{2022} 1x", two), None);
    }

    #[test]
    fn a_version_2_session_takes_its_computed_title() {
        let replay = replay(&[
            r#"{"kind":0,"v":{"version":2,"computedTitle":"Draft notes","requests":[]}}"#
                .to_owned(),
        ]);
        assert_eq!(replay.title().as_deref(), Some("Draft notes"));
    }

    fn user_data() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn finds_logs_ahead_of_whole_files_in_workspaces_and_empty_windows() {
        let root = user_data();
        let storage = root.path().join(USER).join(WORKSPACE_STORAGE);
        let line = format!("{}\n", session_line(&[prompt("1", 10, 1.0)]));
        write(
            &storage.join("abc").join(CHAT_SESSIONS).join("s1.jsonl"),
            &line,
        );
        write(
            &storage.join("abc").join(CHAT_SESSIONS).join("s1.json"),
            "{}",
        );
        write(
            &storage.join("abc").join(CHAT_SESSIONS).join("s2.json"),
            "{}",
        );
        write(
            &storage.join("abc").join(CHAT_SESSIONS).join("notes.txt"),
            "",
        );
        write(&storage.join("def").join("state.vscdb"), "");
        write(
            &root
                .path()
                .join(USER)
                .join(GLOBAL_STORAGE)
                .join(EMPTY_WINDOW_SESSIONS)
                .join("s3.jsonl"),
            &line,
        );

        let mut found: Vec<(String, String)> = find(root.path())
            .into_iter()
            .map(|transcript| {
                assert_eq!(transcript.agent, Agent::CopilotVscode);
                let file = transcript
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                (transcript.session_id, file)
            })
            .collect();
        found.sort();
        assert_eq!(
            found,
            [
                ("s1".to_owned(), "s1.jsonl".to_owned()),
                ("s2".to_owned(), "s2.json".to_owned()),
                ("s3".to_owned(), "s3.jsonl".to_owned()),
            ]
        );
        assert_eq!(find(&root.path().join("nowhere")), []);
    }

    #[test]
    fn reads_a_log_up_to_its_last_whole_line_with_its_workspace_folder() {
        let root = user_data();
        let workspace = root.path().join(USER).join(WORKSPACE_STORAGE).join("abc");
        write(
            &workspace.join(WORKSPACE_FILE),
            r#"{"folder":"file:///c%3A/Users/ada/code/my%20app"}"#,
        );
        let log = workspace.join(CHAT_SESSIONS).join("s1.jsonl");
        let first = session_line(&[prompt("1", 10, 1.0)]);
        write(
            &log,
            &format!(
                "{first}\nnot json\n{}",
                r#"{"kind":1,"k":["requests",0,"completionTokens"],"v":99}"#
            ),
        );

        let (read, details) = super::read(&log).unwrap();
        assert_eq!(read.unreadable, 1);
        assert_eq!(read.end, (first.len() + "\nnot json\n".len()) as u64);
        let Record::Request(request) = &read.records[0] else {
            panic!("{:?}", read.records);
        };
        assert_eq!(
            request.usage[0].tokens.output, 10,
            "the unfinished line waits"
        );
        assert_eq!(details.cwd.as_deref(), Some(r"C:\Users\ada\code\my app"));
        assert_eq!(details.title.as_deref(), Some("Fix the importer"));
    }

    #[test]
    fn reads_a_whole_session_file_and_an_empty_window_has_no_folder() {
        let root = user_data();
        let file = root
            .path()
            .join(USER)
            .join(GLOBAL_STORAGE)
            .join(EMPTY_WINDOW_SESSIONS)
            .join("s1.json");
        let line = session_line(&[prompt("1", 10, 1.0)]);
        let whole = line
            .strip_prefix(r#"{"kind":0,"v":"#)
            .and_then(|rest| rest.strip_suffix('}'))
            .unwrap();
        write(&file, whole);

        let (read, details) = super::read(&file).unwrap();
        assert_eq!(read.records.len(), 1);
        assert_eq!(read.unreadable, 0);
        assert_eq!(details.cwd, None);
    }

    #[test]
    fn folder_uris_become_the_paths_the_agents_write() {
        assert_eq!(
            uri_path("file:///c%3A/Users/ada/code/app").as_deref(),
            Some(r"C:\Users\ada\code\app")
        );
        assert_eq!(uri_path("file:///d%3A/").as_deref(), Some(r"D:\"));
        assert_eq!(
            uri_path("file:///home/ada/code/app/").as_deref(),
            Some("/home/ada/code/app")
        );
        assert_eq!(
            uri_path("file://fileserver/home/ada/app").as_deref(),
            Some(r"\\fileserver\home\ada\app")
        );
        assert_eq!(
            uri_path("vscode-remote://wsl%2Bubuntu/home/ada/app").as_deref(),
            Some("/home/ada/app")
        );
        assert_eq!(
            uri_path("vscode-remote://dev-container%2B7b22/workspaces/app").as_deref(),
            Some("/workspaces/app")
        );
        assert_eq!(uri_path("vscode-vfs://github/ada/app"), None);
        assert_eq!(uri_path("not a uri"), None);
    }

    #[test]
    fn a_multi_root_workspace_takes_its_file_s_folder() {
        assert_eq!(
            workspace_folder(r"C:\Users\ada\code\all.code-workspace").as_deref(),
            Some(r"C:\Users\ada\code")
        );
        assert_eq!(
            workspace_folder(r"C:\all.code-workspace").as_deref(),
            Some(r"C:\")
        );
        assert_eq!(
            workspace_folder("/home/ada/all.code-workspace").as_deref(),
            Some("/home/ada")
        );
        assert_eq!(
            workspace_folder(r"C:\Users\ada\AppData\Roaming\Code\Workspaces\17\workspace.json"),
            None
        );
    }
}
