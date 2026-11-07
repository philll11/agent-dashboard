//! Adopting the history of claude-dashboard, as agent-dashboard was called before version 2
//! (ADR 0014): the first time agent-dashboard opens its own history, it starts from a copy of
//! the one claude-dashboard kept, so no request recorded there is lost. The copy is never written
//! back, and the predecessor's file is only ever read.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use super::{BUSY_TIMEOUT, History, HistoryError, migrations};
use crate::limits::{LimitSnapshot, Source};

impl History {
    /// Brings in the plan-limit readings claude-dashboard recorded in its history at `predecessor`
    /// since this history was adopted from it, and says how many: until Claude Code's status line
    /// runs agent-dashboard, claude-dashboard's copy of the program keeps recording there. None
    /// when this history was not adopted or `predecessor` is gone.
    ///
    /// # Errors
    ///
    /// [`HistoryError::ReadOnly`] once a newer agent-dashboard has upgraded the history, and file
    /// or SQLite errors.
    pub fn bring_in_limits(&mut self, predecessor: &Path) -> Result<usize, HistoryError> {
        let Some(adopted) = self.import(PREDECESSOR)? else {
            return Ok(0);
        };
        if !predecessor.is_file() {
            return Ok(0);
        }
        let source = Connection::open_with_flags(
            predecessor,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        source.busy_timeout(BUSY_TIMEOUT)?;
        migrations::check_owner(&source, predecessor)?;
        let mut statement = source.prepare(
            "SELECT window, source, observed_ms, used_percentage, resets_at_ms
             FROM limit_snapshots WHERE observed_ms >= ?1",
        )?;
        let mut rows = statement.query([adopted.imported_ms])?;
        let mut snapshots = Vec::new();
        while let Some(row) = rows.next()? {
            if let Some(source) = Source::from_name(&row.get::<_, String>(1)?) {
                snapshots.push(LimitSnapshot {
                    window: row.get(0)?,
                    source,
                    observed_ms: row.get(2)?,
                    used_percentage: row.get(3)?,
                    resets_at_ms: row.get(4)?,
                });
            }
        }
        self.write(|w| {
            for snapshot in &snapshots {
                w.limit_snapshot(snapshot)?;
            }
            Ok(())
        })?;
        Ok(snapshots.len())
    }
}

/// The `imports` source that records an adopted history.
pub const PREDECESSOR: &str = "claude-dashboard";

/// Where claude-dashboard kept its history: the folder `claude-dashboard` beside agent-dashboard's
/// own in the operating system's folder for local app data. `None` when the operating system names
/// no such folder.
#[must_use]
pub fn predecessor_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|folder| folder.join(PREDECESSOR).join("history.sqlite"))
}

/// Copies the history at `predecessor` to `path` when `path` does not exist yet, and says how many
/// requests the copy holds; `None` when nothing was copied.
///
/// Every process that opens the default history comes here first, and they take turns: the
/// desktop app opens it from several threads at once, and VS Code runs a server of its own. The
/// copy is made beside `path` and linked into place, so `path` appears complete or not at all.
pub(super) fn adopt(path: &Path, predecessor: &Path) -> Result<Option<u64>, HistoryError> {
    if path.exists() || !predecessor.is_file() {
        return Ok(None);
    }
    let lock_path = path.with_extension("adopt.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(file_error(&lock_path))?;
    lock.lock().map_err(file_error(&lock_path))?;
    if path.exists() {
        return Ok(None);
    }
    copy(predecessor, path).map_err(|source| HistoryError::Predecessor {
        path: predecessor.to_owned(),
        source: Box::new(source),
    })
}

fn copy(predecessor: &Path, path: &Path) -> Result<Option<u64>, HistoryError> {
    let source = Connection::open_with_flags(
        predecessor,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    source.busy_timeout(BUSY_TIMEOUT)?;
    migrations::check_owner(&source, predecessor)?;
    let schema: u32 = source.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if schema == 0 {
        // An empty file: there is nothing to bring in.
        return Ok(None);
    }
    let requests: u64 = source.query_row("SELECT count(*) FROM requests", [], |row| row.get(0))?;

    let copy = path.with_extension("adopting.sqlite");
    match fs::remove_file(&copy) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            return Err(file_error(&copy)(error));
        }
        _ => {}
    }
    let target = copy.to_str().ok_or_else(|| {
        file_error(&copy)(io::Error::new(
            io::ErrorKind::InvalidFilename,
            "the path is not valid Unicode",
        ))
    })?;
    // A read-only connection copies the last committed state, even while claude-dashboard records.
    source.execute("VACUUM INTO ?1", [target])?;
    drop(source);
    // A link, unlike a rename, never replaces a file that is already there.
    let linked = fs::hard_link(&copy, path).map_err(file_error(path));
    let removed = fs::remove_file(&copy).map_err(file_error(&copy));
    linked?;
    removed?;
    Ok(Some(requests))
}

fn file_error(path: &Path) -> impl Fn(io::Error) -> HistoryError + '_ {
    move |source| HistoryError::File {
        path: path.to_owned(),
        source,
    }
}
