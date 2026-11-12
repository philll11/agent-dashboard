//! Adopting claude-dashboard's history: the first open of agent-dashboard's own history starts
//! from a copy of it, once, and never changes the original.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;

use agent_dashboard_core::agent::Agent;
use agent_dashboard_core::history::{
    Attribution, History, HistoryError, Identity, PREDECESSOR, Request, SessionKey,
};
use agent_dashboard_core::limits::{LimitSnapshot, Source};
use agent_dashboard_core::tokens::Tokens;

fn request(session: SessionKey, record_id: &str) -> Request<'_> {
    Request {
        session,
        identity: Identity::Record,
        record_id,
        subagent_id: None,
        timestamp_ms: 1_791_590_400_000,
        model: "claude-sonnet-5-5",
        tokens: Tokens {
            input: 12,
            output: 300,
            cache_read: 4_000,
            cache_write_5m: 300,
            cache_write_1h: 0,
        },
        web_search_requests: 0,
        web_fetch_requests: 0,
        speed: Some("standard"),
        service_tier: Some("standard"),
        inference_geo: None,
        attribution: Attribution::default(),
        entrypoint: Some("cli"),
        agent_version: Some("2.1.296"),
        reasoning_tokens: 0,
        initiator: None,
        charge: None,
        per_prompt: false,
        imported_from: None,
    }
}

/// A claude-dashboard history at `<dir>/claude-dashboard/history.sqlite` with `requests` requests.
fn predecessor(dir: &Path, requests: usize) -> PathBuf {
    let path = dir.join("claude-dashboard").join("history.sqlite");
    let mut history = History::open(&path).unwrap();
    let ids: Vec<String> = (0..requests).map(|n| format!("rec-{n}")).collect();
    history
        .write(|w| {
            let project = w.project("C--code-app", Some(r"C:\code\app"))?;
            let session = w.session("session-1", project, Agent::ClaudeCode, None)?;
            ids.iter()
                .try_for_each(|id| w.request(&request(session, id)))
        })
        .unwrap();
    path
}

fn ours(dir: &Path) -> PathBuf {
    dir.join("agent-dashboard").join("history.sqlite")
}

fn snapshot(observed_ms: i64, used_percentage: f64) -> LimitSnapshot {
    LimitSnapshot {
        window: "five_hour".to_owned(),
        source: Source::StatusLine,
        observed_ms,
        used_percentage,
        resets_at_ms: Some(observed_ms + 3_600_000),
        used: None,
        entitlement: None,
    }
}

#[test]
fn the_first_open_starts_from_a_copy_of_the_predecessors_history() {
    let dir = tempfile::tempdir().unwrap();
    let predecessor = predecessor(dir.path(), 3);
    let before = fs::read(&predecessor).unwrap();

    let history = History::open_adopting(&ours(dir.path()), Some(&predecessor)).unwrap();

    assert_eq!(history.counts().unwrap().requests, 3);
    let adopted = history
        .import(PREDECESSOR)
        .unwrap()
        .expect("recorded as an import");
    assert_eq!(adopted.requests, 3);
    assert_eq!(
        fs::read(&predecessor).unwrap(),
        before,
        "the original is never written"
    );
    assert!(
        !ours(dir.path()).with_extension("adopting.sqlite").exists(),
        "the copy in progress is cleaned up"
    );
}

#[test]
fn a_history_that_exists_is_never_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let predecessor = predecessor(dir.path(), 3);
    let mut history = History::open_adopting(&ours(dir.path()), Some(&predecessor)).unwrap();
    history
        .write(|w| {
            let project = w.project("C--code-other", Some(r"C:\code\other"))?;
            let session = w.session("session-2", project, Agent::ClaudeCode, None)?;
            w.request(&request(session, "rec-new"))
        })
        .unwrap();
    drop(history);

    let history = History::open_adopting(&ours(dir.path()), Some(&predecessor)).unwrap();

    assert_eq!(history.counts().unwrap().requests, 4);
    assert_eq!(history.import(PREDECESSOR).unwrap().unwrap().requests, 3);
}

#[test]
fn processes_opening_at_once_adopt_the_history_once() {
    let dir = tempfile::tempdir().unwrap();
    let predecessor = predecessor(dir.path(), 5);
    let path = ours(dir.path());
    let start = Arc::new(Barrier::new(8));
    let openers: Vec<_> = (0..8)
        .map(|_| {
            let (path, predecessor, start) = (path.clone(), predecessor.clone(), start.clone());
            thread::spawn(move || {
                start.wait();
                History::open_adopting(&path, Some(&predecessor))
                    .unwrap()
                    .counts()
                    .unwrap()
                    .requests
            })
        })
        .collect();

    for opener in openers {
        assert_eq!(opener.join().unwrap(), 5);
    }
    let history = History::open(&path).unwrap();
    assert_eq!(history.import(PREDECESSOR).unwrap().unwrap().requests, 5);
}

#[test]
fn without_a_predecessor_the_history_starts_empty() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("claude-dashboard").join("history.sqlite");

    let history = History::open_adopting(&ours(dir.path()), Some(&missing)).unwrap();

    assert_eq!(history.counts().unwrap().requests, 0);
    assert!(history.import(PREDECESSOR).unwrap().is_none());
}

#[test]
fn a_predecessor_that_is_not_a_history_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let predecessor = dir.path().join("claude-dashboard").join("history.sqlite");
    fs::create_dir_all(predecessor.parent().unwrap()).unwrap();
    let other = rusqlite::Connection::open(&predecessor).unwrap();
    other.execute("CREATE TABLE notes (text TEXT)", []).unwrap();
    drop(other);

    let error = History::open_adopting(&ours(dir.path()), Some(&predecessor)).unwrap_err();

    assert!(
        matches!(&error, HistoryError::Predecessor { path, .. } if *path == predecessor),
        "{error:?}"
    );
    assert!(!ours(dir.path()).exists(), "nothing is left half adopted");
}

#[test]
fn readings_claude_dashboard_records_after_the_adoption_are_brought_in() {
    let dir = tempfile::tempdir().unwrap();
    let predecessor = predecessor(dir.path(), 1);
    let mut history = History::open_adopting(&ours(dir.path()), Some(&predecessor)).unwrap();
    let adopted_ms = history.import(PREDECESSOR).unwrap().unwrap().imported_ms;
    let mut old = History::open(&predecessor).unwrap();
    old.write(|w| {
        w.limit_snapshot(&snapshot(adopted_ms - 600_000, 10.0))?;
        w.limit_snapshot(&snapshot(adopted_ms + 600_000, 42.0))
    })
    .unwrap();
    drop(old);

    let brought = history.bring_in_limits(&predecessor).unwrap();

    assert_eq!(brought, 1, "only readings since the adoption");
    let latest = history.latest_limits().unwrap();
    assert_eq!(latest.len(), 1);
    assert!((latest[0].used_percentage - 42.0).abs() < f64::EPSILON);
}

#[test]
fn a_history_that_was_not_adopted_brings_in_no_readings() {
    let dir = tempfile::tempdir().unwrap();
    let predecessor = predecessor(dir.path(), 1);
    let mut history = History::open_adopting(&ours(dir.path()), None).unwrap();

    assert_eq!(history.bring_in_limits(&predecessor).unwrap(), 0);
}
