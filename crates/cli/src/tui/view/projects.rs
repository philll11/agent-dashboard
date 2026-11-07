//! Projects: each project's sessions, last activity, requests, tokens, daily cost and cost, for the
//! chosen days and model, with the selected project's cost per day by model below. While usage
//! splits by agent, each agent's projects come under its own row of their totals.

use agent_dashboard_server::fmt::{clock, compact, day_label, grouped, money};
use agent_dashboard_server::protocol::{Overview, ProjectRow};
use jiff::Timestamp;

use super::chart;
use super::overview::{agent_chart, cost_chart, cost_text, daily_cost, tokens_of};
use super::{agent_name, agent_parts, agent_series, footer, header, notice};
use crate::tui::canvas::{Canvas, clean, fit, spark};
use crate::tui::state::{Section, State, day};

/// Projects with a smaller share of the cost than this are grouped together.
const SMALL_SHARE: f64 = 0.02;

/// A row of the table: an agent with the totals of its projects, while usage splits by agent;
/// one project; or the small ones together. A project and the small ones carry their agent's id
/// while usage splits by agent.
pub enum Row<'a> {
    Agent(&'a str, &'a Overview),
    Project(&'a ProjectRow, Option<&'a str>),
    Others(Vec<&'a ProjectRow>, Option<&'a str>),
}

/// The table's rows: the projects, highest cost first, with those under 2% of every project's
/// cost together when there are several; each agent's under its own row while usage splits by
/// agent, with those under 2% of that agent's cost together, since each agent may be paid for by
/// someone else.
pub fn rows(state: &State) -> Vec<Row<'_>> {
    let Some(overview) = state
        .projects_request()
        .and_then(|request| state.overview(&request))
    else {
        return Vec::new();
    };
    let total = overview.totals.cost.total;
    if !state.by_agent() {
        return project_rows(&overview.projects, total, None);
    }
    let Some(parts) = state.agent_overviews(state.projects_request()) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for (agent, part) in parts {
        rows.push(Row::Agent(agent, part));
        rows.extend(project_rows(
            &part.projects,
            part.totals.cost.total,
            Some(agent),
        ));
    }
    rows
}

/// The rows of `projects`, of `agent` where usage splits by agent, with those under 2% of `total`
/// together when there are several.
fn project_rows<'a>(
    projects: &'a [ProjectRow],
    total: f64,
    agent: Option<&'a str>,
) -> Vec<Row<'a>> {
    let small: Vec<&ProjectRow> = projects
        .iter()
        .filter(|row| total > 0.0 && row.totals.cost.total / total < SMALL_SHARE)
        .collect();
    if small.len() > 1 {
        let mut rows: Vec<Row<'_>> = projects
            .iter()
            .filter(|row| !small.iter().any(|other| std::ptr::eq(*other, *row)))
            .map(|row| Row::Project(row, agent))
            .collect();
        rows.push(Row::Others(small, agent));
        rows
    } else {
        projects
            .iter()
            .map(|row| Row::Project(row, agent))
            .collect()
    }
}

/// What the chart under the table shows for the selected row: a project, of an agent while usage
/// splits by agent; an agent's projects, for its row or its small projects together; or every
/// project, as ids.
pub fn selected_for_chart(state: &State) -> (Option<String>, Option<String>) {
    match rows(state).get(state.projects.selected) {
        Some(Row::Project(row, agent)) => (Some(row.project.id.clone()), agent.map(str::to_owned)),
        Some(Row::Agent(agent, _) | Row::Others(_, Some(agent))) => {
            (None, Some((*agent).to_owned()))
        }
        _ => (None, None),
    }
}

/// The table's height, box included.
fn table_height(state: &State) -> i32 {
    if state.wide() { 12 } else { 11 }
}

/// How many rows the table shows at once.
pub fn visible_rows(state: &State) -> usize {
    usize::try_from(table_height(state) - 4).unwrap_or(1).max(1)
}

/// A row's figures, for one project, several together, or an agent's.
struct Figures {
    name: String,
    sessions: u64,
    last_ms: i64,
    requests: u64,
    tokens: u64,
    cost: String,
    daily: Vec<(jiff::civil::Date, f64)>,
}

fn figures(state: &State, row: &Row<'_>) -> Figures {
    let projects: Vec<&ProjectRow> = match row {
        Row::Agent(_, overview) => overview.projects.iter().collect(),
        Row::Project(project, _) => vec![*project],
        Row::Others(projects, _) => projects.clone(),
    };
    let mut daily: std::collections::HashMap<jiff::civil::Date, f64> =
        std::collections::HashMap::new();
    for project in &projects {
        for cost in &project.daily_cost {
            *daily.entry(day(&cost.date)).or_default() += cost.usd;
        }
    }
    Figures {
        name: match row {
            Row::Agent(agent, _) => agent_name(state, agent),
            Row::Project(project, _) => clean(&project.project.name),
            Row::Others(..) => "Other projects".to_owned(),
        },
        sessions: projects.iter().map(|project| project.sessions).sum(),
        last_ms: projects
            .iter()
            .map(|project| project.last_ms)
            .max()
            .unwrap_or(0),
        requests: projects.iter().map(|project| project.totals.requests).sum(),
        tokens: projects
            .iter()
            .map(|project| tokens_of(&project.totals))
            .sum(),
        cost: match row {
            Row::Agent(_, overview) => cost_text(&overview.totals),
            Row::Project(project, _) => cost_text(&project.totals),
            Row::Others(projects, _) => money(
                projects
                    .iter()
                    .map(|project| project.totals.cost.total)
                    .sum(),
            ),
        },
        daily: daily.into_iter().collect(),
    }
}

/// Each day's cost in the characters of `ramp`, in at most `cells` columns: days added together
/// when there are more days than columns.
fn trend(
    figures: &Figures,
    (first, last): (jiff::civil::Date, jiff::civil::Date),
    cells: usize,
    ramp: &[&str; 8],
) -> String {
    let days = daily_cost(figures.daily.iter().copied(), first, last);
    if days.len() <= cells || cells == 0 {
        return spark(&days, ramp);
    }
    let per = days.len().div_ceil(cells);
    let grouped: Vec<f64> = days.chunks(per).map(|chunk| chunk.iter().sum()).collect();
    spark(&grouped, ramp)
}

/// Where the table's columns are. They keep the boards' places at 120 and 80 columns; a wider
/// window widens the daily cost while the range has more days than it shows, and the cost stays
/// beside it.
struct Columns {
    wide: bool,
    sessions_end: i32,
    cost_end: i32,
    trend_x: i32,
    trend_cells: usize,
}

fn columns(state: &State, w: i32) -> Columns {
    let wide = state.wide();
    let days = state.days().map_or(30, |days| {
        (day(&days.last) - day(&days.first)).get_days() + 1
    });
    if wide {
        let trend_x = 78;
        let trend_cells = days.min(w - 2 - 10 - trend_x).max(0);
        Columns {
            wide,
            sessions_end: 36,
            cost_end: (trend_x + trend_cells.max(30) + 10).min(w - 2),
            trend_x,
            trend_cells: usize::try_from(trend_cells).unwrap_or(0),
        }
    } else {
        let trend_x = 44;
        Columns {
            wide,
            sessions_end: 30,
            cost_end: 41,
            trend_x,
            trend_cells: usize::try_from((w - 3 - trend_x).max(0)).unwrap_or(0),
        }
    }
}

pub fn draw(c: &mut Canvas, state: &State) {
    header(c, state, Some(Section::Projects));
    notice(c, state, "", false);
    let rows = rows(state);
    table(c, state, &rows);
    chart_below(c, state, &rows);
    footer(
        c,
        state,
        &[
            ("q", "quit"),
            ("←→", "sections"),
            ("Enter", "filter"),
            ("t", "table view"),
            ("e", "export"),
            ("?", "help"),
        ],
        None,
    );
}

/// The table of projects, with the selected one marked and a total.
fn table(c: &mut Canvas, state: &State, rows: &[Row<'_>]) {
    let theme = c.theme;
    let w = c.w;
    let columns = columns(state, w);
    c.boxed(
        (0, 2, w, table_height(state)),
        Some(&format!("Projects · {}", state.range.title())),
        columns
            .wide
            .then(|| ("Enter filters the dashboard to one", theme.muted())),
    );
    let heading = 3;
    c.text(4, heading, "Project", theme.muted());
    c.right(columns.sessions_end, heading, "Sessions", theme.muted());
    c.right(columns.cost_end, heading, "Cost", theme.muted());
    c.text(columns.trend_x, heading, "Daily cost", theme.muted());
    if columns.wide {
        c.text(40, heading, "Last active", theme.muted());
        c.right(66, heading, "Requests", theme.muted());
        c.right(75, heading, "Tokens", theme.muted());
    }
    let visible = visible_rows(state);
    let top = state.projects.top.min(rows.len().saturating_sub(visible));
    for (index, row) in (0..).zip(rows.iter().skip(top).take(visible)) {
        let y = heading + 1 + index;
        project_line(c, state, row, y, &columns);
        if top + usize::try_from(index).unwrap_or(0) == state.projects.selected {
            c.select(y, 1, w - 2);
        }
    }
    let overview = state
        .projects_request()
        .and_then(|request| state.overview(&request));
    let Some(overview) = overview
        .filter(|_| !state.by_agent() || state.agent_overviews(state.projects_request()).is_some())
    else {
        c.text(4, heading + 1, "Loading the projects…", theme.muted());
        return;
    };
    let projects: Vec<&Row<'_>> = rows
        .iter()
        .filter(|row| !matches!(row, Row::Agent(..)))
        .collect();
    if projects.is_empty() {
        c.text(4, heading + 1, "No projects in these days.", theme.muted());
        return;
    }
    let all: Vec<Figures> = projects.iter().map(|row| figures(state, row)).collect();
    let y = heading + 1 + i32::try_from(rows.len().min(visible)).unwrap_or(0);
    c.text(4, y, "Total", theme.strong());
    c.right(
        columns.sessions_end,
        y,
        &grouped(all.iter().map(|row| row.sessions).sum()),
        theme.text(),
    );
    if columns.wide {
        c.right(
            66,
            y,
            &grouped(all.iter().map(|row| row.requests).sum()),
            theme.text(),
        );
        c.right(
            75,
            y,
            &compact(all.iter().map(|row| row.tokens).sum()),
            theme.text(),
        );
    }
    c.right(
        columns.cost_end,
        y,
        &cost_text(&overview.totals),
        theme.strong(),
    );
}

/// A row's line: an agent's under its mark, and the projects an agent's own a step in from it.
fn project_line(c: &mut Canvas, state: &State, row: &Row<'_>, y: i32, columns: &Columns) {
    let theme = c.theme;
    let figures = figures(state, row);
    let (x, style) = match row {
        Row::Agent(agent, _) => {
            let series = agent_series(agent);
            c.put(4, y, theme.swatch(series), theme.series(series));
            (6, theme.strong())
        }
        Row::Project(_, agent) => (if agent.is_some() { 6 } else { 4 }, theme.text()),
        Row::Others(_, agent) => (if agent.is_some() { 6 } else { 4 }, theme.muted()),
    };
    c.text(
        x,
        y,
        &fit(&figures.name, columns.sessions_end - 9 - x),
        style,
    );
    c.right(
        columns.sessions_end,
        y,
        &grouped(figures.sessions),
        theme.text(),
    );
    if columns.wide {
        let last = Timestamp::from_millisecond(figures.last_ms)
            .map(|at| {
                let at = at.to_zoned(state.zone());
                format!("{} {}", day_label(at.date()), clock(&at))
            })
            .unwrap_or_default();
        c.text(40, y, &last, theme.muted());
        c.right(66, y, &grouped(figures.requests), theme.text());
        c.right(75, y, &compact(figures.tokens), theme.text());
    }
    if let Some(days) = state.days() {
        let days = (day(&days.first), day(&days.last));
        c.text(
            columns.trend_x,
            y,
            &trend(&figures, days, columns.trend_cells, theme.ramp()),
            theme.accent(),
        );
    }
    c.right(columns.cost_end, y, &figures.cost, theme.strong());
}

/// The selected project's cost per day by model; an agent's projects' while its row or its small
/// projects together are selected, while usage splits by agent; and otherwise every project's, by
/// agent while usage splits by agent.
fn chart_below(c: &mut Canvas, state: &State, rows: &[Row<'_>]) {
    let table_h = table_height(state);
    let area = (0, 2 + table_h, c.w, c.h - 3 - table_h);
    let name = match rows.get(state.projects.selected) {
        // Both agents may work in a project of the same name.
        Some(Row::Project(project, Some(agent))) => format!(
            "{} · {}",
            clean(&project.project.name),
            agent_name(state, agent)
        ),
        Some(Row::Project(project, None)) => clean(&project.project.name),
        Some(Row::Agent(agent, _) | Row::Others(_, Some(agent))) => agent_name(state, agent),
        _ => "All projects".to_owned(),
    };
    let Some(overview) = state
        .project_chart_request()
        .and_then(|request| state.overview(&request))
    else {
        return;
    };
    let every = matches!(selected_for_chart(state), (None, None));
    let chart = match agent_parts(state, state.projects_request()) {
        Some(parts) if every => agent_chart(state, overview, &parts, c.w - 10),
        _ => cost_chart(state, overview, c.w - 10),
    };
    let title = format!(
        "{name} · cost per {} by {} · {}",
        chart.per(),
        chart.by(),
        state.range.title()
    );
    if state.tables {
        chart::table(c, area, &chart, &title, state.table_top);
    } else {
        chart::draw(c, area, &chart, &title);
    }
}
