//! `agent-dashboard export`, and the reports' JSON and CSV, which come from the same export as
//! the dashboard's files.

use std::io::{self, BufWriter, IsTerminal, Write};
use std::ops::ControlFlow;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use agent_dashboard_core::agent::Agent;
use agent_dashboard_server::app::App;
use agent_dashboard_server::export;
use agent_dashboard_server::fmt::grouped;
use agent_dashboard_server::protocol::{Dataset, Days, ExportFormat, ExportParams, Filters};
use jiff::Timestamp;
use jiff::civil::Date;

use crate::report::{Given, Output, colour_json, ready_history};
use crate::style::Paint;

/// How often an export to a file shows its progress on a terminal.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// The filters of a report or export that counts only `agent`'s requests, or every agent's.
fn filters(agent: Option<Agent>) -> Filters {
    Filters {
        agent: agent.map(|agent| agent.as_str().to_owned()),
        ..Filters::default()
    }
}

/// What a report's `--json` or `--csv` writes: `dataset` for the days from `first` to `last`, of
/// `agent`'s requests or every agent's.
pub fn params(
    dataset: Dataset,
    output: Output,
    (first, last): (Date, Date),
    agent: Option<Agent>,
) -> ExportParams {
    ExportParams {
        dataset,
        format: if output == Output::Json {
            ExportFormat::Json
        } else {
            ExportFormat::Csv
        },
        days: Some(Days {
            first: first.to_string(),
            last: last.to_string(),
        }),
        filters: filters(agent),
        search: None,
        session: None,
    }
}

/// Prints an export to standard output, its JSON coloured as the design shows it when standard
/// output is a terminal.
pub fn print(app: &App, params: &ExportParams) -> ExitCode {
    let paint = Paint::for_stdout();
    let mut quiet = |_, _| ControlFlow::Continue(());
    let result = if params.format == ExportFormat::Json && paint != Paint::PLAIN {
        let mut json = Vec::new();
        export::write(app, params, &mut json, false, &mut quiet).map(|_| {
            println!(
                "{}",
                colour_json(String::from_utf8_lossy(&json).trim_end(), paint)
            );
        })
    } else {
        let mut out = BufWriter::new(io::stdout().lock());
        export::write(app, params, &mut out, false, &mut quiet).and_then(|_| {
            out.flush().map_err(|error| {
                export::ExportError::Failed(format!("Cannot write the export: {error}"))
            })
        })
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// `agent-dashboard export`: every request, or those of the days given, of `agent` or every
/// agent, as CSV or JSON, to standard output or to a file saved for a spreadsheet.
pub fn run(
    app: &App,
    (since, until): Given,
    agent: Option<Agent>,
    format: ExportFormat,
    output: Option<&Path>,
) -> ExitCode {
    let (app, history) = match ready_history(app) {
        Ok(ready) => ready,
        Err(code) => return code,
    };
    let app = &app;
    let days = if since.is_some() || until.is_some() {
        let today = app.now_zoned().date();
        let last = until.unwrap_or(today);
        let first = since.unwrap_or_else(|| {
            history
                .first_request_ms()
                .ok()
                .flatten()
                .and_then(|ms| Timestamp::from_millisecond(ms).ok())
                .map_or(last, |at| at.to_zoned(app.zone.clone()).date())
                .min(last)
        });
        Some(Days {
            first: first.to_string(),
            last: last.to_string(),
        })
    } else {
        None
    };
    drop(history);
    let params = ExportParams {
        dataset: Dataset::Requests,
        format,
        days,
        filters: filters(agent),
        search: None,
        session: None,
    };
    let Some(path) = output else {
        return print(app, &params);
    };
    let show = io::stderr().is_terminal();
    let mut shown = Instant::now();
    let mut progress = |written: u64, total: u64| {
        if show && shown.elapsed() >= PROGRESS_EVERY {
            shown = Instant::now();
            let mut stderr = io::stderr();
            let _ = write!(
                stderr,
                "\rExporting: {} of {} requests",
                grouped(written),
                grouped(total)
            );
            let _ = stderr.flush();
        }
        ControlFlow::Continue(())
    };
    let saved = export::save(app, &params, path, &mut progress);
    if show {
        let _ = write!(io::stderr(), "\r{}\r", " ".repeat(60));
    }
    match saved {
        Ok(exported) => {
            println!("Saved {} to {}", exported.what, path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
