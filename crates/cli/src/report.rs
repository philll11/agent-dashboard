//! `today`, `daily`, `weekly` and `monthly`: requests, tokens and cost, as tables, JSON or CSV.

use std::fmt::Write as _;
use std::process::ExitCode;

use agent_dashboard_core::agent::{Agent, Plan};
use agent_dashboard_core::history::{History, HistoryError};
use agent_dashboard_core::limits::{FIVE_HOUR, Level, LimitSnapshot, SEVEN_DAY, Source};
use agent_dashboard_core::report::{
    self, Bucket, CostProvenance, DayTotals, Filter, ModelTotals, Range, Totals,
};
use jiff::civil::Date;
use jiff::{Timestamp, ToSpan};
use serde::Serialize;

use agent_dashboard_server::app::App;
use agent_dashboard_server::export::{self, DayRow};
use agent_dashboard_server::fmt::{
    ago, compact, grouped, long_day, money, month_year, short_date, weekday_name, when,
};
use agent_dashboard_server::limits::login_note;
use agent_dashboard_server::names::{model_name, name_under};
use agent_dashboard_server::protocol::{Dataset, ExportFormat};

use crate::limits;
use crate::progress;
use crate::style::Paint;

/// How a report is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Table,
    Json,
    Csv,
}

/// The days a report is given: the first and the last, either of which may be left out.
pub type Given = (Option<Date>, Option<Date>);

/// Which requests a report counts: `agent`'s, or every agent's.
pub fn only(agent: Option<Agent>) -> Filter {
    Filter {
        agent,
        ..Filter::default()
    }
}

/// Where a report's costs come from: its totals', or, while none of its requests is priced,
/// where the agents it covers take theirs from.
pub fn provenance(history: &History, agent: Option<Agent>, totals: &Totals) -> CostProvenance {
    if let Some(provenance) = totals.cost_provenance() {
        return provenance;
    }
    let agents = agents_in_view(history, agent);
    match (
        agents.iter().any(|agent| !agent.records_charges()),
        agents.iter().any(|agent| agent.records_charges()),
    ) {
        (true, true) => CostProvenance::Mixed,
        (false, true) => CostProvenance::Recorded,
        _ => CostProvenance::Estimate,
    }
}

/// What a report's costs are, for the line above its table: `API-equivalent estimate · prices of
/// 10 Oct 2026`, `recorded cost`, or `cost includes estimate · prices of 10 Oct 2026`.
pub fn cost_note(app: &App, provenance: CostProvenance) -> String {
    let prices = short_date(app.prices.checked());
    match provenance {
        CostProvenance::Estimate => format!("API-equivalent estimate · prices of {prices}"),
        CostProvenance::Recorded => "recorded cost".to_owned(),
        CostProvenance::Mixed => format!("cost includes estimate · prices of {prices}"),
    }
}

/// Totals added up.
pub fn added<'a>(totals: impl IntoIterator<Item = &'a Totals>) -> Totals {
    let mut sum = Totals::default();
    for totals in totals {
        sum.merge(totals);
    }
    sum
}

/// How much of an allowance is used, where the agent records it: `1,575 of 45,000 AI credits`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn credits(used: Option<f64>, entitlement: Option<f64>) -> Option<String> {
    let whole = |credits: f64| grouped(credits.round().max(0.0) as u64);
    Some(format!(
        "{} of {} AI credits",
        whole(used?),
        whole(entitlement?)
    ))
}

/// Opens and ingests the history, printing why when it cannot; with the app as the history then
/// sets it up, so that the names the agents record for models just read are known.
pub fn ready_history(app: &App) -> Result<(App, History), ExitCode> {
    let mut history = app.open_history().map_err(|error| {
        eprintln!("{error}");
        ExitCode::FAILURE
    })?;
    if let Err(error) = progress::ingest(app, &mut history) {
        eprintln!("Cannot read new transcripts into the history: {error}");
    }
    Ok((app.with_shared(&history), history))
}

pub fn today(app: &App, agent: Option<Agent>, output: Output) -> ExitCode {
    let (app, mut history) = match ready_history(app) {
        Ok(ready) => ready,
        Err(code) => return code,
    };
    let app = &app;
    let claude_code = agent.is_none_or(|agent| agent == Agent::ClaudeCode);
    if claude_code {
        limits::refresh(app, &mut history);
    }
    let day = app.now_zoned().date();
    let range = Range::days(day, day, &app.zone);
    let models = match report::by_model(&history, &app.prices, range, &only(agent)) {
        Ok(models) => models,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let limits: Vec<LimitSnapshot> = history
        .latest_limits()
        .unwrap_or_default()
        .into_iter()
        .filter(|limit| agent.is_none_or(|agent| limit.source.plan() == agent.plan()))
        .collect();
    match output {
        Output::Table => {
            let paint = Paint::for_stdout();
            let mut out = String::new();
            let provenance = provenance(&history, agent, &sum(&models));
            let _ = writeln!(
                out,
                "{}",
                paint.muted(&format!(
                    "{} · {}",
                    long_day(day),
                    cost_note(app, provenance)
                ))
            );
            out.push('\n');
            if models.is_empty() {
                let _ = writeln!(out, "{}", paint.muted("No requests today."));
            } else {
                let split = split_agents(&history, agent);
                let groups = match models_by_agent(app, &history, range, &split) {
                    Ok(groups) => groups,
                    Err(error) => {
                        eprintln!("Cannot read the history: {error}");
                        return ExitCode::FAILURE;
                    }
                };
                out.push_str(&model_table(app, paint, &models, &groups));
            }
            out.push('\n');
            let agents = agents_in_view(&history, agent);
            let _ = writeln!(out, "{}", limits_line(app, paint, &limits, &agents));
            let endpoint = history.usage_endpoint().unwrap_or_default();
            if let Some(note) = login_note(&endpoint).filter(|_| claude_code) {
                let _ = writeln!(out, "{}", paint.paint(note, paint.palette().warning, false));
            }
            print!("{out}");
        }
        Output::Json => {
            let totals = DayTotals {
                day,
                totals: sum(&models),
                models: models.clone(),
            };
            let today = TodayJson {
                day: DayRow::of(app, &totals),
                limits: limits
                    .iter()
                    .filter(|limit| !limit.expired(app.now.as_millisecond()))
                    .map(limit_json)
                    .collect(),
            };
            print_json(&today);
        }
        Output::Csv => {
            let totals = DayTotals {
                day,
                totals: sum(&models),
                models,
            };
            let mut csv = Vec::new();
            let _ = export::write_days(app, &[totals], ExportFormat::Csv, &mut csv);
            print!("{}", String::from_utf8_lossy(&csv));
        }
    }
    ExitCode::SUCCESS
}

pub fn daily(app: &App, (since, until): Given, agent: Option<Agent>, output: Output) -> ExitCode {
    let (app, history) = match ready_history(app) {
        Ok(ready) => ready,
        Err(code) => return code,
    };
    let app = &app;
    let today = app.now_zoned().date();
    let until = until.unwrap_or(today);
    let since = since.unwrap_or_else(|| {
        until
            .checked_sub(jiff::Span::new().days(29))
            .unwrap_or(until)
    });
    if output != Output::Table {
        drop(history);
        return crate::export::print(
            app,
            &crate::export::params(Dataset::Days, output, (since, until), agent),
        );
    }
    let days = match report::by_day(
        &history,
        &app.prices,
        Range::days(since, until, &app.zone),
        &app.zone,
        &only(agent),
    ) {
        Ok(days) => days,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let total = added(days.iter().map(|day| &day.totals));
    let note = cost_note(app, provenance(&history, agent, &total));
    print!("{}", days_table(Paint::for_stdout(), &note, &days));
    ExitCode::SUCCESS
}

/// `weekly` and `monthly`: requests, tokens and cost per week or per calendar month. Weeks start
/// on the day the shared settings name. Without days given, the last 12 weeks or months; with
/// them, every week or month they touch, whole.
pub fn periods(
    app: &App,
    bucket: Bucket,
    (since, until): Given,
    agent: Option<Agent>,
    output: Output,
) -> ExitCode {
    let (app, history) = match ready_history(app) {
        Ok(ready) => ready,
        Err(code) => return code,
    };
    let app = &app;
    let last_start = bucket.start(
        until.unwrap_or_else(|| app.now_zoned().date()),
        app.week_start,
    );
    let first = since.map_or_else(
        || {
            let back = match bucket {
                Bucket::Month => 11.months(),
                Bucket::Week | Bucket::Day => 77.days(),
            };
            last_start.checked_sub(back).unwrap_or(last_start)
        },
        |since| bucket.start(since, app.week_start),
    );
    let last = bucket.end(last_start);
    if output != Output::Table {
        drop(history);
        let dataset = if bucket == Bucket::Month {
            Dataset::Months
        } else {
            Dataset::Weeks
        };
        return crate::export::print(
            app,
            &crate::export::params(dataset, output, (first, last), agent),
        );
    }
    let days = match report::by_day(
        &history,
        &app.prices,
        Range::days(first, last, &app.zone),
        &app.zone,
        &only(agent),
    ) {
        Ok(days) => days,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let periods = report::by_bucket(&days, bucket, app.week_start);
    let total = added(periods.iter().map(|period| &period.totals));
    let note = cost_note(app, provenance(&history, agent, &total));
    let note = if bucket == Bucket::Week {
        format!("Weeks from {} · {note}", weekday_name(app.week_start))
    } else {
        note
    };
    print!(
        "{}",
        periods_table(Paint::for_stdout(), &note, bucket, &periods)
    );
    ExitCode::SUCCESS
}

fn periods_table(paint: Paint, note: &str, bucket: Bucket, periods: &[DayTotals]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", paint.muted(note));
    out.push('\n');
    if periods.is_empty() {
        let _ = writeln!(out, "{}", paint.muted("No requests on these days."));
        return out;
    }
    let total = added(periods.iter().map(|period| &period.totals));
    let rows: Vec<Row> = periods
        .iter()
        .map(|period| {
            let label = match bucket {
                Bucket::Month => month_year(period.day),
                Bucket::Week | Bucket::Day => short_date(period.day),
            };
            Row {
                label_len: label.chars().count(),
                label,
                values: cells(&period.totals),
            }
        })
        .collect();
    let heading = match bucket {
        Bucket::Month => "Month",
        Bucket::Week | Bucket::Day => "Week of",
    };
    out.push_str(&totals_table(paint, heading, &rows, &total));
    out
}

pub fn sum(models: &[ModelTotals]) -> Totals {
    added(models.iter().map(|model| &model.totals))
}

/// The columns of the design's tables: a label, then right-aligned numbers. The widths are the
/// design's; a column widens to keep two spaces before its longest value.
const HEADINGS: [&str; 5] = ["Requests", "Output", "Cache writes", "Cache reads", "Cost"];
const WIDTHS: [usize; 5] = [8, 10, 15, 14, 9];
const LABEL_WIDTH: usize = 14;

/// A row's values; the cache tokens of a row of prompts alone, which VS Code does not record,
/// show as `—`.
pub fn cells(totals: &Totals) -> [String; 5] {
    let cost = if totals.requests > 0 && totals.unpriced_requests == totals.requests {
        "unpriced".to_owned()
    } else {
        money(totals.cost.total())
    };
    let prompts = totals.requests > 0 && totals.per_prompt_requests == totals.requests;
    let recorded = |tokens: u64| {
        if prompts {
            "—".to_owned()
        } else {
            compact(tokens)
        }
    };
    [
        grouped(totals.requests),
        compact(totals.tokens.output),
        recorded(totals.tokens.cache_write_5m + totals.tokens.cache_write_1h),
        recorded(totals.tokens.cache_read),
        cost,
    ]
}

/// A row of a table: its label as written, with the length it shows at, and its values.
pub struct Row {
    pub label: String,
    pub label_len: usize,
    pub values: [String; 5],
}

/// A table with a heading row, its rows and a total row in bold.
pub fn table(paint: Paint, first_heading: &str, rows: &[Row], total: &Row) -> String {
    let all = || rows.iter().chain(std::iter::once(total));
    let label_width = all()
        .map(|row| row.label_len + 2)
        .fold(LABEL_WIDTH, usize::max);
    let mut widths = WIDTHS;
    for (index, width) in widths.iter_mut().enumerate() {
        let longest = all()
            .map(|row| row.values[index].chars().count() + 2)
            .max()
            .unwrap_or(0);
        *width = (*width).max(longest);
    }
    let render = |label: &str, label_len: usize, values: &[&str]| {
        let mut line = String::from(label);
        line.push_str(&" ".repeat(label_width.saturating_sub(label_len)));
        for (value, width) in values.iter().zip(widths) {
            let _ = write!(line, "{value:>width$}");
        }
        line
    };
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}",
        paint.muted(&render(
            first_heading,
            first_heading.chars().count(),
            &HEADINGS
        ))
    );
    for row in rows {
        let values: Vec<&str> = row.values.iter().map(String::as_str).collect();
        let _ = writeln!(out, "{}", render(&row.label, row.label_len, &values));
    }
    let values: Vec<&str> = total.values.iter().map(String::as_str).collect();
    let _ = writeln!(
        out,
        "{}",
        paint.strong(&render(&total.label, total.label_len, &values))
    );
    out
}

pub fn total_row(totals: &Totals) -> Row {
    Row {
        label: "  Total".to_owned(),
        label_len: 7,
        values: cells(totals),
    }
}

/// A table whose last row adds up `totals`, with a note under it when some of them are prompts
/// from VS Code's chat session files, which record a prompt's output tokens but not its cache
/// tokens.
pub fn totals_table(paint: Paint, first_heading: &str, rows: &[Row], totals: &Totals) -> String {
    let mut out = table(paint, first_heading, rows, &total_row(totals));
    let prompts = totals.per_prompt_requests;
    if prompts > 0 {
        let _ = writeln!(
            out,
            "{}",
            paint.muted(&format!(
                "Cache tokens leave out {} {} from VS Code's chat session files, which record output tokens alone.",
                grouped(prompts),
                if prompts == 1 { "prompt" } else { "prompts" }
            ))
        );
    }
    out
}

/// Each model's row, marked with its series.
pub fn model_rows(app: &App, paint: Paint, models: &[ModelTotals]) -> Vec<Row> {
    models
        .iter()
        .map(|model| {
            let name = model_name(app, &model.model);
            Row {
                label: format!(
                    "{} {name}",
                    paint.paint("■", paint.palette().series(&model.model), false)
                ),
                label_len: name.chars().count() + 2,
                values: cells(&model.totals),
            }
        })
        .collect()
}

/// The agents a report splits its models and projects between, so that one agent's are never
/// added to another's: every agent the history holds, while it holds more than one and the
/// report is not of one agent.
pub fn split_agents(history: &History, agent: Option<Agent>) -> Vec<Agent> {
    if agent.is_some() {
        return Vec::new();
    }
    let agents = history.agents().unwrap_or_default();
    if agents.len() > 1 { agents } else { Vec::new() }
}

/// An agent's row of the totals of its models or projects, which come after it.
pub fn agent_row(paint: Paint, agent: Agent, totals: &Totals) -> Row {
    let name = agent.display_name();
    Row {
        label: paint.strong(name),
        label_len: name.chars().count(),
        values: cells(totals),
    }
}

/// A row of a model or project under its agent's row, a step in from it.
pub fn under_agent(name: &str, totals: &Totals) -> Row {
    Row {
        label: format!("  {name}"),
        label_len: name.chars().count() + 2,
        values: cells(totals),
    }
}

/// Each agent's row, then its models under it, without the models' series marks, which colour a
/// model whichever agent used it.
pub fn agent_model_rows(app: &App, paint: Paint, groups: &[(Agent, Vec<ModelTotals>)]) -> Vec<Row> {
    let mut rows = Vec::new();
    for (agent, models) in groups {
        rows.push(agent_row(paint, *agent, &sum(models)));
        rows.extend(models.iter().map(|model| {
            let name = model_name(app, &model.model);
            under_agent(name_under(*agent, &name), &model.totals)
        }));
    }
    rows
}

/// Each of `agents`' models in `range`, highest cost first.
///
/// # Errors
///
/// When the history cannot be read.
pub fn models_by_agent(
    app: &App,
    history: &History,
    range: Range,
    agents: &[Agent],
) -> Result<Vec<(Agent, Vec<ModelTotals>)>, HistoryError> {
    agents
        .iter()
        .map(|agent| {
            let models = report::by_model(history, &app.prices, range, &only(Some(*agent)))?;
            Ok((*agent, models))
        })
        .collect()
}

/// Today's models: each agent's under its row when `groups` splits them by agent.
fn model_table(
    app: &App,
    paint: Paint,
    models: &[ModelTotals],
    groups: &[(Agent, Vec<ModelTotals>)],
) -> String {
    let rows = if groups.is_empty() {
        model_rows(app, paint, models)
    } else {
        agent_model_rows(app, paint, groups)
    };
    let total = sum(models);
    let mut out = totals_table(paint, "Model", &rows, &total);
    if total.unpriced_requests > 0 && total.unpriced_requests < total.requests {
        let _ = writeln!(
            out,
            "{}",
            paint.muted(&format!(
                "The total leaves out {} {} to unpriced models.",
                grouped(total.unpriced_requests),
                if total.unpriced_requests == 1 {
                    "request"
                } else {
                    "requests"
                }
            ))
        );
    }
    out
}

fn days_table(paint: Paint, note: &str, days: &[DayTotals]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", paint.muted(note));
    out.push('\n');
    if days.is_empty() {
        let _ = writeln!(out, "{}", paint.muted("No requests on these days."));
        return out;
    }
    let total = added(days.iter().map(|day| &day.totals));
    let rows: Vec<Row> = days
        .iter()
        .map(|day| {
            let label = format!(
                "{} {:>2} {}",
                &format!("{:?}", day.day.weekday())[..3],
                day.day.day(),
                short_date(day.day).split(' ').nth(1).unwrap_or_default()
            );
            Row {
                label_len: label.chars().count(),
                label,
                values: cells(&day.totals),
            }
        })
        .collect();
    out.push_str(&totals_table(paint, "Day", &rows, &total));
    out
}

fn window_name(window: &str) -> String {
    match window {
        FIVE_HOUR => "five-hour".to_owned(),
        SEVEN_DAY => "weekly".to_owned(),
        other => agent_dashboard_core::limits::weekly_model(other).map_or_else(
            || other.replace('_', " "),
            |model| format!("weekly {model}"),
        ),
    }
}

/// The plan-limits lines under a report, as the design writes them: Claude Code's windows on one
/// line, and Copilot's allowance on the next. A line shows for the plan of each of `agents`, the
/// agents the report covers, and for any other plan whose readings `limits` holds; Claude's shows
/// while no agent is known yet.
pub fn limits_line(app: &App, paint: Paint, limits: &[LimitSnapshot], agents: &[Agent]) -> String {
    let (copilot, claude): (Vec<&LimitSnapshot>, Vec<&LimitSnapshot>) = limits
        .iter()
        .partition(|limit| limit.source.plan() == Plan::Copilot);
    let covers = |plan: Plan| agents.iter().any(|agent| agent.plan() == plan);
    let mut lines = Vec::new();
    if agents.is_empty() || covers(Plan::Claude) || !claude.is_empty() {
        lines.push(plan_limits_line(app, paint, &claude));
    }
    if covers(Plan::Copilot) || !copilot.is_empty() {
        lines.push(allowance_line(app, paint, &copilot));
    }
    lines.join("\n")
}

/// The agents a report covers: `agent`, or every agent the history holds.
pub fn agents_in_view(history: &History, agent: Option<Agent>) -> Vec<Agent> {
    agent.map_or_else(|| history.agents().unwrap_or_default(), |agent| vec![agent])
}

/// How old the newest of `limits` is, in the warning colour when one of them is stale.
fn reading_age(app: &App, paint: Paint, limits: &[&LimitSnapshot]) -> String {
    let now_ms = app.now.as_millisecond();
    let latest = limits
        .iter()
        .map(|limit| limit.observed_ms)
        .max()
        .unwrap_or(now_ms);
    let age = ago(
        Timestamp::from_millisecond(latest).unwrap_or(app.now),
        app.now,
    );
    if limits.iter().any(|limit| limit.stale(now_ms)) {
        paint.paint(&age, paint.palette().warning, false)
    } else {
        paint.muted(&age)
    }
}

/// A window's use, marked from 70%.
fn figure(paint: Paint, limit: &LimitSnapshot) -> String {
    let percent = format!("{:.0}%", limit.used_percentage);
    match limit.level() {
        Level::Normal => paint.strong(&percent),
        Level::Warning => paint.paint(&format!("{percent} ▲"), paint.palette().warning, true),
        // Red passes WCAG AA only for marks, so the figure stays in strong ink.
        Level::Critical => format!(
            "{} {}",
            paint.strong(&percent),
            paint.paint("▲", paint.palette().critical, true)
        ),
    }
}

/// When a window resets: ` resets Sun 11 Oct`; nothing without a reset time.
fn resets_when(app: &App, limit: &LimitSnapshot) -> String {
    let now = app.now_zoned();
    limit
        .resets_at_ms
        .and_then(|ms| Timestamp::from_millisecond(ms).ok())
        .map(|at| format!(" resets {}", when(&at.to_zoned(app.zone.clone()), &now)))
        .unwrap_or_default()
}

/// Claude Code's plan windows: `Plan limits, official, 2 min ago: five-hour 42% resets 14:20`.
fn plan_limits_line(app: &App, paint: Paint, limits: &[&LimitSnapshot]) -> String {
    let now_ms = app.now.as_millisecond();
    let current: Vec<&LimitSnapshot> = limits
        .iter()
        .copied()
        .filter(|limit| !limit.expired(now_ms))
        .collect();
    if current.is_empty() {
        return if limits.is_empty() {
            format!(
                "{} {}",
                paint.muted("Plan limits: not recorded yet. To record them, run"),
                paint.strong("agent-dashboard statusline connect")
            )
        } else {
            paint.muted("Plan limits: the windows have reset since the last reading.")
        };
    }
    let source = if current
        .iter()
        .all(|limit| limit.source == Source::StatusLine)
    {
        "official"
    } else {
        "experimental"
    };
    let windows: Vec<String> = current
        .iter()
        .map(|limit| {
            format!(
                "{} {}{}",
                window_name(&limit.window),
                figure(paint, limit),
                resets_when(app, limit)
            )
        })
        .collect();
    format!(
        "{} {}{} {}",
        paint.muted(&format!("Plan limits, {source},")),
        reading_age(app, paint, &current),
        paint.muted(":"),
        windows.join(" · ")
    )
}

/// Copilot's monthly allowance: `Copilot allowance, recorded, 1 h ago: 4% · 1,575 of 45,000 AI
/// credits · resets 1 Dec 13:00`.
fn allowance_line(app: &App, paint: Paint, limits: &[&LimitSnapshot]) -> String {
    let now_ms = app.now.as_millisecond();
    let Some(limit) = limits.iter().max_by_key(|limit| limit.observed_ms) else {
        return paint.muted(
            "Copilot allowance: none recorded. Copilot CLI and VS Code's Copilot harness record it as they run, on a plan that has one.",
        );
    };
    if limit.expired(now_ms) {
        return paint.muted("Copilot allowance: the month has reset since the last reading.");
    }
    let mut parts = vec![figure(paint, limit)];
    parts.extend(credits(limit.used, limit.entitlement));
    let resets = resets_when(app, limit);
    if !resets.is_empty() {
        parts.push(resets.trim_start().to_owned());
    }
    format!(
        "{} {}{} {}",
        paint.muted("Copilot allowance, recorded,"),
        reading_age(app, paint, &[limit]),
        paint.muted(":"),
        parts.join(" · ")
    )
}

#[derive(Serialize)]
struct TodayJson {
    #[serde(flatten)]
    day: DayRow,
    limits: Vec<LimitJson>,
}

#[derive(Serialize)]
struct LimitJson {
    plan: &'static str,
    window: String,
    used_percentage: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    used_credits: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entitlement_credits: Option<f64>,
    resets_at: Option<String>,
    observed_at: String,
    provenance: &'static str,
}

fn limit_json(limit: &LimitSnapshot) -> LimitJson {
    let at = |ms: i64| {
        Timestamp::from_millisecond(ms)
            .map(|at| at.to_string())
            .ok()
    };
    LimitJson {
        plan: limit.source.plan().as_str(),
        window: limit.window.clone(),
        used_percentage: limit.used_percentage,
        used_credits: limit.used,
        entitlement_credits: limit.entitlement,
        resets_at: limit.resets_at_ms.and_then(at),
        observed_at: at(limit.observed_ms).unwrap_or_default(),
        provenance: match limit.source {
            Source::StatusLine => "official",
            Source::CopilotSessionFiles => "recorded",
            Source::UsageEndpoint => "experimental",
        },
    }
}

/// Prints JSON, coloured as the design shows it when standard output is a terminal.
pub fn print_json(value: &impl Serialize) {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    println!("{}", colour_json(&text, Paint::for_stdout()));
}

/// Colours pretty-printed JSON: keys in the accent colour and string values in the string
/// colour.
pub fn colour_json(text: &str, paint: Paint) -> String {
    if paint == Paint::PLAIN {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() * 2);
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if c != '"' {
            out.push(c);
            continue;
        }
        let mut end = start + 1;
        let mut escaped = false;
        for (index, c) in chars.by_ref() {
            end = index + c.len_utf8();
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                break;
            }
        }
        let string = &text[start..end];
        let is_key = text[end..].trim_start().starts_with(':');
        let colour = if is_key {
            paint.palette().accent
        } else {
            paint.palette().string
        };
        out.push_str(&paint.paint(string, colour, false));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(label: &str, values: [&str; 5]) -> Row {
        Row {
            label: label.to_owned(),
            label_len: label.chars().count(),
            values: values.map(str::to_owned),
        }
    }

    #[test]
    fn rows_line_up_under_their_headings_as_in_the_design() {
        let rows = [row("■ Fable 5.1", ["16", "3K", "9K", "166K", "$1.83"])];
        let total = row("  Total", ["113", "13K", "39K", "1.0M", "$5.05"]);
        assert_eq!(
            table(Paint::PLAIN, "Model", &rows, &total),
            [
                "Model         Requests    Output   Cache writes   Cache reads     Cost",
                "■ Fable 5.1         16        3K             9K          166K    $1.83",
                "  Total            113       13K            39K          1.0M    $5.05",
                "",
            ]
            .join("\n")
        );
    }

    #[test]
    fn a_wide_value_widens_its_column() {
        let rows = [row(
            "Sun  4 Oct",
            ["27,627", "20.4M", "106M", "7.1B", "$2,372.37"],
        )];
        let total = row("  Total", ["27,627", "20.4M", "106M", "7.1B", "$2,372.37"]);
        let text = table(Paint::PLAIN, "Day", &rows, &total);
        assert!(text.contains("7.1B  $2,372.37"), "{text}");
        let widths: Vec<usize> = text.lines().map(|line| line.chars().count()).collect();
        assert!(widths.iter().all(|width| *width == widths[0]), "{text}");
    }

    #[test]
    fn json_keys_and_strings_take_the_design_colours() {
        let plain = r#"{ "date": "2026-10-05", "requests": 151 }"#;
        assert_eq!(colour_json(plain, Paint::PLAIN), plain);
    }
}
