//! `models`, `sessions` and `projects`: where the requests of a stretch of days went. While the
//! history holds more than one agent and a report is not of one, each agent's models and projects
//! come under its own row, never added to another's.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::process::ExitCode;

use agent_dashboard_core::agent::Agent;
use agent_dashboard_core::history::{History, HistoryError, ProjectKey, SessionKey};
use agent_dashboard_core::report::{self, Range};
use jiff::Span;
use jiff::civil::Date;

use agent_dashboard_server::app::App;
use agent_dashboard_server::fmt::short_date;
use agent_dashboard_server::names::project_names;
use agent_dashboard_server::protocol::Dataset;

use crate::report::{
    Given, Output, Row, added, agent_model_rows, agent_row, cells, cost_note, model_rows,
    models_by_agent, only, provenance, ready_history, split_agents, sum, totals_table, under_agent,
};
use crate::style::Paint;

/// The longest label a session or project gets in a table.
const LABEL_LIMIT: usize = 40;

/// The days to report on: `days` days up to today unless given.
fn days(app: &App, (since, until): Given, days: i64) -> (Date, Date) {
    let until = until.unwrap_or_else(|| app.now_zoned().date());
    let since = since.unwrap_or_else(|| {
        until
            .checked_sub(Span::new().days(days - 1))
            .unwrap_or(until)
    });
    (since, until)
}

/// The line above a table: its days, and what its costs are.
fn heading(
    app: &App,
    (since, until): (Date, Date),
    (history, agent): (&History, Option<Agent>),
    totals: &report::Totals,
) -> String {
    format!(
        "{} to {} · {}",
        short_date(since),
        short_date(until),
        cost_note(app, provenance(history, agent, totals))
    )
}

/// `text` cut to `limit` characters, ending in an ellipsis when cut.
fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_owned()
    } else {
        let mut short: String = text.chars().take(limit - 1).collect();
        short.push('…');
        short
    }
}

pub fn models(app: &App, given: Given, agent: Option<Agent>, output: Output) -> ExitCode {
    let (app, history) = match ready_history(app) {
        Ok(ready) => ready,
        Err(code) => return code,
    };
    let app = &app;
    let (since, until) = days(app, given, 30);
    let split = split_agents(&history, agent);
    if output != Output::Table {
        drop(history);
        let params = crate::export::params(Dataset::Models, output, (since, until), agent);
        return crate::export::print(app, &params);
    }
    let range = Range::days(since, until, &app.zone);
    let (models, groups) = match report::by_model(&history, &app.prices, range, &only(agent))
        .and_then(|models| Ok((models, models_by_agent(app, &history, range, &split)?)))
    {
        Ok(found) => found,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let paint = Paint::for_stdout();
    let heading = heading(app, (since, until), (&history, agent), &sum(&models));
    let mut out = format!("{}\n\n", paint.muted(&heading));
    if models.is_empty() {
        let _ = writeln!(out, "{}", paint.muted("No requests on these days."));
    } else {
        let rows = if groups.is_empty() {
            model_rows(app, paint, &models)
        } else {
            agent_model_rows(app, paint, &groups)
        };
        out.push_str(&totals_table(paint, "Model", &rows, &sum(&models)));
    }
    print!("{out}");
    ExitCode::SUCCESS
}

/// A session in a report.
struct SessionView {
    session_id: String,
    title: Option<String>,
    project: String,
    first_ms: i64,
    totals: report::Totals,
}

pub fn sessions(app: &App, given: Given, agent: Option<Agent>, output: Output) -> ExitCode {
    let (app, history) = match ready_history(app) {
        Ok(ready) => ready,
        Err(code) => return code,
    };
    let app = &app;
    let (since, until) = days(app, given, 7);
    if output != Output::Table {
        drop(history);
        return crate::export::print(
            app,
            &crate::export::params(Dataset::Sessions, output, (since, until), agent),
        );
    }
    let sessions = match report::by_session(
        &history,
        &app.prices,
        Range::days(since, until, &app.zone),
        &only(agent),
    ) {
        Ok(sessions) => sessions,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let details: HashMap<SessionKey, (String, Option<String>)> = history
        .sessions()
        .unwrap_or_default()
        .into_iter()
        .map(|session| (session.key, (session.session_id, session.title)))
        .collect();
    let projects = project_names(&history);
    let mut views: Vec<SessionView> = sessions
        .into_iter()
        .map(|session| {
            let (session_id, title) = details.get(&session.session).cloned().unwrap_or_default();
            SessionView {
                session_id,
                title,
                project: projects
                    .get(&session.project)
                    .map(|(name, _, _)| name.clone())
                    .unwrap_or_default(),
                first_ms: session.first_ms,
                totals: session.totals,
            }
        })
        .collect();
    // As the dashboard and the exports list them: the session started last first.
    views.sort_by(|a, b| {
        b.first_ms
            .cmp(&a.first_ms)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    let totals = added(views.iter().map(|session| &session.totals));
    let heading = heading(app, (since, until), (&history, agent), &totals);
    print!("{}", sessions_table(&heading, &views, &totals));
    ExitCode::SUCCESS
}

fn sessions_table(heading: &str, sessions: &[SessionView], totals: &report::Totals) -> String {
    let paint = Paint::for_stdout();
    let mut out = format!("{}\n\n", paint.muted(heading));
    if sessions.is_empty() {
        let _ = writeln!(out, "{}", paint.muted("No sessions on these days."));
        return out;
    }
    let rows: Vec<Row> = sessions
        .iter()
        .map(|session| {
            let title = session
                .title
                .clone()
                .unwrap_or_else(|| session.session_id.chars().take(8).collect());
            let label = shorten(&format!("{title} · {}", session.project), LABEL_LIMIT);
            Row {
                label_len: label.chars().count(),
                label,
                values: cells(&session.totals),
            }
        })
        .collect();
    out.push_str(&totals_table(paint, "Session", &rows, totals));
    out
}

pub fn projects(app: &App, given: Given, agent: Option<Agent>, output: Output) -> ExitCode {
    let (app, history) = match ready_history(app) {
        Ok(ready) => ready,
        Err(code) => return code,
    };
    let app = &app;
    let (since, until) = days(app, given, 30);
    let split = split_agents(&history, agent);
    if output != Output::Table {
        drop(history);
        let params = crate::export::params(Dataset::Projects, output, (since, until), agent);
        return crate::export::print(app, &params);
    }
    let range = Range::days(since, until, &app.zone);
    let projects_of =
        |agent: Option<Agent>| report::by_project(&history, &app.prices, range, &only(agent));
    let found = projects_of(agent).and_then(|totals| {
        let groups = split
            .iter()
            .map(|agent| projects_of(Some(*agent)).map(|projects| (*agent, projects)))
            .collect::<Result<Vec<_>, HistoryError>>()?;
        Ok((totals, groups))
    });
    let (totals, groups) = match found {
        Ok(found) => found,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let names = project_names(&history);
    let name = |key: ProjectKey| names.get(&key).cloned().unwrap_or_default();
    let paint = Paint::for_stdout();
    let all = added(totals.iter().map(|project| &project.totals));
    let heading = heading(app, (since, until), (&history, agent), &all);
    let mut out = format!("{}\n\n", paint.muted(&heading));
    if totals.is_empty() {
        let _ = writeln!(out, "{}", paint.muted("No requests on these days."));
    } else if groups.is_empty() {
        let rows: Vec<Row> = totals
            .iter()
            .map(|project| {
                let label = shorten(&name(project.project).0, LABEL_LIMIT);
                Row {
                    label_len: label.chars().count(),
                    label,
                    values: cells(&project.totals),
                }
            })
            .collect();
        out.push_str(&totals_table(paint, "Project", &rows, &all));
    } else {
        let mut rows = Vec::new();
        for (agent, projects) in &groups {
            rows.push(agent_row(
                paint,
                *agent,
                &added(projects.iter().map(|project| &project.totals)),
            ));
            rows.extend(projects.iter().map(|project| {
                under_agent(
                    &shorten(&name(project.project).0, LABEL_LIMIT - 2),
                    &project.totals,
                )
            }));
        }
        out.push_str(&totals_table(paint, "Project", &rows, &all));
    }
    print!("{out}");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::shorten;

    #[test]
    fn long_labels_end_in_an_ellipsis() {
        assert_eq!(shorten("Fix the importer", 40), "Fix the importer");
        assert_eq!(shorten("abcdefghij", 6), "abcde…");
    }
}
