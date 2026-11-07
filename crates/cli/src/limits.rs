//! `agent-dashboard limits`: each agent's plan windows and Claude Code's recent five-hour
//! windows; turning Anthropic's usage endpoint on or off; and what `doctor` says about it.

use std::fmt::Write as _;
use std::process::ExitCode;

use agent_dashboard_core::agent::Agent;
use agent_dashboard_core::history::History;
use agent_dashboard_core::usage_endpoint::{self, Outcome, Refresh, State};
use agent_dashboard_server::app::App;
use agent_dashboard_server::export::{WindowRow, csv_field};
use agent_dashboard_server::fmt::{ago, clock, day_label, money, percent, resets};
use agent_dashboard_server::glance::{in_order, window_name};
use agent_dashboard_server::limits::{CONSENT, login_note, sentence, trouble, view};
use agent_dashboard_server::protocol::{
    FiveHourWindow, Level, Limits, Provenance, WindowReading, WindowView,
};
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use serde::Serialize;

use crate::prompt;
use crate::report::{Output, credits, limits_line, print_json, ready_history};
use crate::style::Paint;

/// What a window is called under its agent's heading, where Copilot CLI's allowance is its
/// `Monthly allowance`.
pub fn grouped_name(reading: &WindowReading) -> String {
    if reading.window == "monthly" {
        "Monthly allowance".to_owned()
    } else {
        window_name(reading)
    }
}

/// The agent a window belongs to.
pub fn agent_of(reading: &WindowReading) -> Option<Agent> {
    Agent::from_name(&reading.agent)
}

/// Each plan window with its use, reset, source and age, and the recent five-hour windows, after
/// giving the usage endpoint its turn.
pub fn report(app: &App, output: Output) -> ExitCode {
    let (app, mut history) = match ready_history(app) {
        Ok(ready) => ready,
        Err(code) => return code,
    };
    let app = &app;
    refresh(app, &mut history);
    let limits = match view(app, &history) {
        Ok(limits) => limits,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let windows = in_order(&limits.windows, |window| &window.reading);
    match output {
        Output::Table => {
            let paint = Paint::for_stdout();
            let agents = history.agents().unwrap_or_default();
            let mut out = limits_table(app, paint, (&limits, &windows), &agents);
            let endpoint = history.usage_endpoint().unwrap_or_default();
            if let Some(note) = login_note(&endpoint) {
                let _ = writeln!(
                    out,
                    "\n{}",
                    paint.paint(note, paint.palette().warning, false)
                );
            }
            print!("{out}");
        }
        Output::Json => print_json(&LimitsJson {
            windows: windows.iter().map(|window| window_json(window)).collect(),
            recent_five_hour_windows: limits
                .recent
                .iter()
                .map(|window| WindowRow::of(app, window))
                .collect(),
        }),
        Output::Csv => {
            // New columns go at the end, so that a script that reads the others by position
            // keeps working.
            let mut out = String::from(
                "window,model,used_percentage,resets_at,observed_at,provenance,stale,expired,agent,used_credits,entitlement_credits\n",
            );
            let number = |value: Option<f64>| value.map(|value| value.to_string());
            for window in &windows {
                let reading = &window.reading;
                let _ = writeln!(
                    out,
                    "{},{},{},{},{},{},{},{},{},{},{}",
                    csv_field(&reading.window),
                    csv_field(reading.model.as_deref().unwrap_or_default()),
                    reading.used_percentage,
                    reading.resets_at_ms.and_then(at).unwrap_or_default(),
                    at(reading.observed_ms).unwrap_or_default(),
                    provenance(reading.provenance),
                    reading.stale,
                    reading.expired,
                    csv_field(&reading.agent),
                    number(reading.used).unwrap_or_default(),
                    number(reading.entitlement).unwrap_or_default()
                );
            }
            print!("{out}");
        }
    }
    ExitCode::SUCCESS
}

fn provenance(provenance: Provenance) -> &'static str {
    match provenance {
        Provenance::Official => "official",
        Provenance::Recorded => "recorded",
        Provenance::Experimental => "experimental",
    }
}

fn at(ms: i64) -> Option<String> {
    Timestamp::from_millisecond(ms)
        .ok()
        .map(|at| at.to_string())
}

fn zoned(app: &App, ms: i64) -> Option<Zoned> {
    Timestamp::from_millisecond(ms)
        .ok()
        .map(|at| at.to_zoned(app.zone.clone()))
}

/// A five-hour window's span: `Sat 10 Oct 09:20–14:20`, or across midnight
/// `Fri 9 Oct 22:00–Sat 10 Oct 03:00`.
pub fn five_hour_span(zone: &TimeZone, window: &FiveHourWindow) -> String {
    let at = |ms: i64| {
        Timestamp::from_millisecond(ms)
            .ok()
            .map(|at| at.to_zoned(zone.clone()))
    };
    let (Some(start), Some(end)) = (at(window.start_ms), at(window.resets_at_ms)) else {
        return String::new();
    };
    let end = if end.date() == start.date() {
        clock(&end)
    } else {
        format!("{} {}", day_label(end.date()), clock(&end))
    };
    format!("{} {}–{end}", day_label(start.date()), clock(&start))
}

/// The windows by agent, each agent under its heading, Claude Code's with its recent five-hour
/// windows. `agents` are those the history holds sessions of: Claude Code's heading says how to
/// record its plan limits while it has none.
fn limits_table(
    app: &App,
    paint: Paint,
    (limits, windows): (&Limits, &[&WindowView]),
    agents: &[Agent],
) -> String {
    if windows.is_empty() {
        return format!("{}\n", limits_line(app, paint, &[], agents));
    }
    let rows: Vec<Columns> = windows.iter().map(|window| columns(app, window)).collect();
    let widths = Widths::of(&rows);
    let mut groups = Vec::new();
    for agent in Agent::ALL {
        let group: Vec<(&WindowView, &Columns)> = windows
            .iter()
            .zip(&rows)
            .filter(|(window, _)| agent_of(&window.reading) == Some(agent))
            .map(|(window, row)| (*window, row))
            .collect();
        let claude_code = agent == Agent::ClaudeCode;
        let mut out = match agent {
            Agent::ClaudeCode => format!(
                "{}{}\n\n",
                paint.strong(agent.display_name()),
                paint.muted(" · limits shared with claude.ai")
            ),
            Agent::CopilotCli => format!("{}\n\n", paint.strong(agent.display_name())),
        };
        if group.is_empty() {
            if claude_code && agents.contains(&agent) {
                let _ = writeln!(out, "{}", limits_line(app, paint, &[], &[agent]));
                groups.push(out);
            }
            continue;
        }
        out.push_str(&windows_table(paint, &group, &widths));
        if claude_code && !limits.recent.is_empty() {
            out.push('\n');
            out.push_str(&recent_table(app, paint, &limits.recent));
        }
        groups.push(out);
    }
    groups.join("\n")
}

/// A window's columns as text: its name, when it resets, where its reading comes from, how old
/// that is, and how many AI credits of the allowance are used, where the agent records them.
struct Columns {
    name: String,
    resets: String,
    source: &'static str,
    age: String,
    credits: Option<String>,
}

fn columns(app: &App, window: &WindowView) -> Columns {
    let reading = &window.reading;
    let now = app.now_zoned();
    let resets = if reading.expired {
        "no reading since it reset".to_owned()
    } else {
        reading
            .resets_at_ms
            .and_then(|ms| zoned(app, ms))
            .map_or_else(
                || "no reset time".to_owned(),
                |at| {
                    let text = resets(&at, &now);
                    text.strip_prefix("Resets ").unwrap_or(&text).to_owned()
                },
            )
    };
    Columns {
        name: grouped_name(reading),
        resets,
        source: provenance(reading.provenance),
        age: Timestamp::from_millisecond(reading.observed_ms)
            .map(|observed| ago(observed, app.now))
            .unwrap_or_default(),
        credits: credits(reading.used, reading.entitlement)
            .map(|text| text.trim_end_matches(" AI credits").to_owned()),
    }
}

/// The widths of the columns every agent's table shares, so that they line up.
struct Widths {
    name: usize,
    resets: usize,
    source: usize,
}

impl Widths {
    fn of(rows: &[Columns]) -> Self {
        let width = |column: fn(&Columns) -> &str, heading: &str| {
            rows.iter()
                .map(|row| column(row).chars().count())
                .chain([heading.chars().count()])
                .max()
                .unwrap_or(0)
        };
        Self {
            name: width(|row| &row.name, "Window"),
            resets: width(|row| &row.resets, "Resets"),
            source: width(|row| row.source, "Source"),
        }
    }
}

/// One agent's windows with their use, reset, source and the age of their readings, and the AI
/// credits used where the agent records them.
fn windows_table(paint: Paint, windows: &[(&WindowView, &Columns)], widths: &Widths) -> String {
    let mut out = String::new();
    let palette = paint.palette();
    let (name_width, resets_width, source_width) = (widths.name, widths.resets, widths.source);
    let with_credits = windows.iter().any(|(_, row)| row.credits.is_some());
    let age_width = windows
        .iter()
        .map(|(_, row)| row.age.chars().count())
        .chain(["Reading".chars().count()])
        .max()
        .unwrap_or(0);
    let mut heading = format!(
        "{:<name_width$}   {:>4}     {:<resets_width$}   {:<source_width$}   ",
        "Window", "Used", "Resets", "Source"
    );
    if with_credits {
        let _ = write!(heading, "{:<age_width$}   AI credits", "Reading");
    } else {
        heading.push_str("Reading");
    }
    let _ = writeln!(out, "{}", paint.muted(&heading));
    for (window, row) in windows {
        let reading = &window.reading;
        let (used, marker) = if reading.expired {
            (paint.muted(&format!("{:>4}", "–")), "  ".to_owned())
        } else {
            let figure = format!("{:>4}", percent(reading.used_percentage));
            match reading.level {
                Level::Normal => (paint.strong(&figure), "  ".to_owned()),
                Level::Warning => (
                    paint.paint(&figure, palette.warning, true),
                    format!(" {}", paint.paint("▲", palette.warning, true)),
                ),
                // Red passes WCAG AA only for marks, so the figure stays in strong ink.
                Level::Critical => (
                    paint.strong(&figure),
                    format!(" {}", paint.paint("▲", palette.critical, true)),
                ),
            }
        };
        let age = if with_credits {
            format!("{:<age_width$}", row.age)
        } else {
            row.age.clone()
        };
        let age = if reading.stale {
            paint.paint(&age, palette.warning, false)
        } else {
            paint.muted(&age)
        };
        let credits = match (&row.credits, with_credits) {
            (Some(credits), true) => format!("   {credits}"),
            _ => String::new(),
        };
        let _ = writeln!(
            out,
            "{:<name_width$}   {used}{marker}   {:<resets_width$}   {}   {age}{credits}",
            row.name,
            row.resets,
            paint.muted(&format!("{:<source_width$}", row.source)),
        );
    }
    out
}

/// The recent five-hour windows, with the highest use a reading saw and their cost.
fn recent_table(app: &App, paint: Paint, recent: &[FiveHourWindow]) -> String {
    let mut out = String::new();
    let palette = paint.palette();
    let spans: Vec<String> = recent
        .iter()
        .map(|window| {
            let running = window.start_ms <= app.now.as_millisecond()
                && app.now.as_millisecond() < window.resets_at_ms;
            format!(
                "{}{}",
                five_hour_span(&app.zone, window),
                if running { ", in progress" } else { "" }
            )
        })
        .collect();
    let span_width = spans
        .iter()
        .map(|span| span.chars().count())
        .chain(["Recent five-hour windows".chars().count()])
        .max()
        .unwrap_or(0);
    let costs: Vec<String> = recent
        .iter()
        .map(|window| {
            let totals = &window.totals;
            if totals.requests > 0 && totals.unpriced_requests == totals.requests {
                "unpriced".to_owned()
            } else {
                money(totals.cost.total)
            }
        })
        .collect();
    let cost_width = costs
        .iter()
        .map(|cost| cost.chars().count())
        .chain(["API-equivalent cost".chars().count()])
        .max()
        .unwrap_or(0);
    let _ = writeln!(
        out,
        "{}",
        paint.muted(&format!(
            "{:<span_width$}   {:>7}   {:>cost_width$}",
            "Recent five-hour windows", "Highest", "API-equivalent cost"
        ))
    );
    for ((window, span), cost) in recent.iter().zip(&spans).zip(&costs) {
        let highest = format!("{:>7}", percent(window.highest));
        let marker = if window.highest >= 90.0 {
            format!(" {}", paint.paint("▲", palette.critical, true))
        } else if window.highest >= 70.0 {
            format!(" {}", paint.paint("▲", palette.warning, true))
        } else {
            "  ".to_owned()
        };
        let _ = writeln!(
            out,
            "{span:<span_width$}   {highest}{marker} {}",
            paint.strong(&format!("{cost:>cost_width$}"))
        );
    }
    out
}

#[derive(Serialize)]
struct LimitsJson {
    windows: Vec<WindowJson>,
    recent_five_hour_windows: Vec<WindowRow>,
}

#[derive(Serialize)]
struct WindowJson {
    agent: String,
    window: String,
    model: Option<String>,
    used_percentage: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    used_credits: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entitlement_credits: Option<f64>,
    resets_at: Option<String>,
    observed_at: Option<String>,
    provenance: &'static str,
    stale: bool,
    expired: bool,
    pace: Option<PaceJson>,
}

#[derive(Serialize)]
struct PaceJson {
    /// The share of the limit used when the window resets, at the pace since `since`.
    at_reset_percentage: f64,
    limit_reached_at: Option<String>,
    since: Option<String>,
}

fn window_json(window: &WindowView) -> WindowJson {
    let reading = &window.reading;
    WindowJson {
        agent: reading.agent.clone(),
        window: reading.window.clone(),
        model: reading.model.clone(),
        used_percentage: reading.used_percentage,
        used_credits: reading.used,
        entitlement_credits: reading.entitlement,
        resets_at: reading.resets_at_ms.and_then(at),
        observed_at: at(reading.observed_ms),
        provenance: provenance(reading.provenance),
        stale: reading.stale,
        expired: reading.expired,
        pace: window.pace.map(|pace| PaceJson {
            at_reset_percentage: pace.at_reset,
            limit_reached_at: pace.limit_reached_ms.and_then(at),
            since: at(pace.since_ms),
        }),
    }
}

pub fn endpoint(app: &App, on: bool, yes: bool) -> ExitCode {
    let mut history = match app.open_history() {
        Ok(history) => history,
        Err(error) => {
            eprintln!("Cannot open the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let enabled = match history.usage_endpoint() {
        Ok(state) => state.enabled,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let paint = Paint::for_stdout();
    if on == enabled {
        println!(
            "The usage endpoint is already {}.",
            if on { "on" } else { "off" }
        );
        return ExitCode::SUCCESS;
    }
    if on {
        println!("{}\n", paint.strong("Use Anthropic's usage endpoint?"));
        for sentence in CONSENT {
            println!("{sentence}");
        }
        if !prompt::confirmed(paint, yes, "Turn it on?") {
            return ExitCode::FAILURE;
        }
    }
    if let Err(error) = history.write(|w| w.set_usage_endpoint(on)) {
        eprintln!("Cannot record the change: {error}");
        return ExitCode::FAILURE;
    }
    if !on {
        println!("The usage endpoint is off.");
        return ExitCode::SUCCESS;
    }
    println!("\nThe usage endpoint is on. Turn it off with agent-dashboard limits endpoint off.");
    // Asking once now shows whether the endpoint works with this computer's login.
    let asked = matches!(refresh(app, &mut history), Some(Refresh::Asked(_)));
    let state = history.usage_endpoint().unwrap_or_default();
    if !asked {
        println!(
            "{}",
            paint.muted("It is asked whenever no plan-limit reading is newer than 5 minutes.")
        );
    } else if state.outcome == Some(Outcome::Answered) {
        let limits: Vec<_> = history
            .latest_limits()
            .unwrap_or_default()
            .into_iter()
            .filter(|limit| limit.source.agent() == Agent::ClaudeCode)
            .collect();
        println!("{}", limits_line(app, paint, &limits, &[Agent::ClaudeCode]));
    } else if let Some(note) = login_note(&state) {
        println!("{}", paint.paint(note, paint.palette().warning, false));
    } else if let Some(trouble) = trouble(app, &state) {
        println!(
            "{}",
            paint.paint(&sentence(&trouble), paint.palette().warning, false)
        );
    }
    ExitCode::SUCCESS
}

/// Asks the usage endpoint for the plan limits when it is on and its turn has come. What comes
/// of it is recorded in the history, so a report never fails over it.
pub fn refresh(app: &App, history: &mut History) -> Option<Refresh> {
    let login = app.login_source()?;
    usage_endpoint::refresh(history, &login, app.now.as_millisecond()).ok()
}

/// The `doctor` line for the endpoint while it is on: whether it works, and what to say.
pub fn doctor_detail(app: &App, state: &State) -> (bool, String) {
    match (state.outcome, state.asked_ms) {
        (Some(Outcome::Answered), Some(asked_ms)) => {
            let asked = Timestamp::from_millisecond(asked_ms)
                .map(|at| ago(at, app.now))
                .unwrap_or_default();
            (
                true,
                format!("usage endpoint on (experimental), last answer {asked}"),
            )
        }
        (Some(_), Some(_)) => (
            false,
            format!(
                "usage endpoint on (experimental): {}",
                trouble(app, state).unwrap_or_default()
            ),
        ),
        _ => (
            true,
            "usage endpoint on (experimental), no answer yet".to_owned(),
        ),
    }
}
