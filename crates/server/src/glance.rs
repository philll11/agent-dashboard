//! The glance: plan limits and today's cost in a few words, for the desktop app's item in the
//! notification area or the menu bar. VS Code's status bar says the same; `design/glance.json`
//! holds the examples both are tested against.

use agent_dashboard_core::history::{History, HistoryError};
use agent_dashboard_core::report::{self, Filter, Range};
use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::app::App;
use crate::fmt::{ago, grouped, money, percent, when};
use crate::protocol::{Level, Provenance, WindowReading};
use crate::requests::window_reading;

/// What the app is called in the notification area, the menu bar and its menu.
pub const NAME: &str = "Dashboard for Coding Agents";

/// What a window is called: `Five-hour window`, `Weekly, all models`, `Weekly, Fable`, or Copilot
/// CLI's `Copilot allowance`.
#[must_use]
pub fn window_name(reading: &WindowReading) -> String {
    match (reading.window.as_str(), &reading.model) {
        ("five_hour", _) => "Five-hour window".to_owned(),
        ("seven_day", _) => "Weekly, all models".to_owned(),
        ("monthly", _) => "Copilot allowance".to_owned(),
        (_, Some(model)) => format!("Weekly, {model}"),
        ("spend_limit", None) => "Spend limit".to_owned(),
        (window, None) => window.to_owned(),
    }
}

/// Claude Code's windows first: five-hour, then weekly, then each model's weekly window, then any
/// other; then Copilot CLI's allowance.
#[must_use]
pub fn in_order<T>(items: &[T], reading: impl Fn(&T) -> &WindowReading) -> Vec<&T> {
    let rank = |reading: &WindowReading| match reading.window.as_str() {
        "five_hour" => 0,
        "seven_day" => 1,
        "monthly" => 4,
        _ if reading.model.is_some() => 2,
        _ => 3,
    };
    let key = |reading: &WindowReading| {
        reading
            .model
            .clone()
            .unwrap_or_else(|| reading.window.clone())
    };
    let mut ordered: Vec<&T> = items.iter().collect();
    ordered.sort_by(|a, b| {
        let (a, b) = (reading(a), reading(b));
        rank(a).cmp(&rank(b)).then_with(|| key(a).cmp(&key(b)))
    });
    ordered
}

/// A line of the glance: a plan window's use, or today's cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// `Five-hour window`, or `Today`.
    pub label: String,
    /// `42% · resets 14:20`, `71% ▲ · resets Thu 09:00`, or `$5.05`.
    pub value: String,
}

impl Line {
    /// The line as a menu writes it: `Five-hour window: 42% · resets 14:20`.
    #[must_use]
    pub fn text(&self) -> String {
        format!("{}: {}", self.label, self.value)
    }
}

/// The use the icon shows: the five-hour window's, while a reading of it is current, at the more
/// urgent level of the five-hour window and Copilot CLI's allowance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Figure {
    /// The share of the limit used, in percent.
    pub used: f64,
    pub level: Level,
}

/// Everything the glance shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Glance {
    /// The figure the icon shows; without a current reading, the icon shows the gauge and the
    /// title today's cost.
    pub figure: Option<Figure>,
    /// A line per plan window, Claude Code's first, then today's cost.
    pub lines: Vec<Line>,
    /// Where each agent's newest reading comes from and how old it is, Claude Code's first:
    /// `Official, status line, 2 min ago`, `Recorded, Copilot CLI, 1 h ago`.
    pub sources: Vec<String>,
    /// The tooltip, in Microsoft's "name - status" form: `Dashboard for Coding Agents - 5h 42% ·
    /// resets 14:20 · Copilot 4%`.
    pub tooltip: String,
    /// The title beside the menu bar symbol on macOS: `42%`, `71% ▲`, or `$5.05`.
    pub title: String,
}

/// Today's cost, as the glance needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Today {
    pub usd: f64,
    pub unpriced_requests: u64,
}

/// The glance, from the history as it is at `app.now`.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn read(app: &App, history: &History) -> Result<Glance, HistoryError> {
    let readings: Vec<WindowReading> = history
        .latest_limits()?
        .iter()
        .map(|reading| window_reading(app, reading))
        .collect();
    let today = app.now_zoned().date();
    let totals = report::totals(
        history,
        &app.prices,
        Range::days(today, today, &app.zone),
        &Filter::default(),
    )?;
    Ok(compose(
        &readings,
        Today {
            usd: totals.cost.total(),
            unpriced_requests: totals.unpriced_requests,
        },
        &app.zone,
        app.now.as_millisecond(),
    ))
}

/// Whether a window has reset since its reading, so that the reading no longer tells its use.
fn expired(reading: &WindowReading, now_ms: i64) -> bool {
    reading.expired
        || reading
            .resets_at_ms
            .is_some_and(|resets_at_ms| resets_at_ms <= now_ms)
}

fn moment(ms: i64, zone: &TimeZone, now_ms: i64) -> String {
    let zoned = |ms: i64| {
        Timestamp::from_millisecond(ms)
            .unwrap_or_default()
            .to_zoned(zone.clone())
    };
    when(&zoned(ms), &zoned(now_ms))
}

/// How much of a window is used, marked from 70%, and when it resets: `71% ▲ · resets Thu 09:00`;
/// for an allowance whose credits are known, how many of them: `4% · 1,608 of 45,000 AI credits`.
fn value(reading: &WindowReading, zone: &TimeZone, now_ms: i64) -> String {
    if expired(reading, now_ms) {
        return reading.resets_at_ms.map_or_else(
            || "reset".to_owned(),
            |resets_at_ms| format!("reset at {}", moment(resets_at_ms, zone, now_ms)),
        );
    }
    let mark = if reading.level == Level::Normal {
        ""
    } else {
        " ▲"
    };
    let used = format!("{}{mark}", percent(reading.used_percentage));
    if let (Some(credits), Some(entitlement)) = (reading.used, reading.entitlement) {
        return format!(
            "{used} · {} of {} AI credits",
            whole(credits),
            whole(entitlement)
        );
    }
    match reading.resets_at_ms {
        Some(resets_at_ms) => format!("{used} · resets {}", moment(resets_at_ms, zone, now_ms)),
        None => used,
    }
}

/// A count of credits, to the whole credit: `45,000`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole(credits: f64) -> String {
    grouped(credits.round().max(0.0) as u64)
}

/// The more urgent of two levels.
fn higher(a: Level, b: Level) -> Level {
    let rank = |level: Level| match level {
        Level::Normal => 0,
        Level::Warning => 1,
        Level::Critical => 2,
    };
    if rank(b) > rank(a) { b } else { a }
}

/// A cost as the dashboard writes it, saying when some requests have no price.
fn cost(today: Today) -> String {
    if today.unpriced_requests > 0 {
        format!("{} + unpriced", money(today.usd))
    } else {
        money(today.usd)
    }
}

/// The glance for `readings` and today's cost, in `zone`, at `now_ms`.
#[must_use]
pub fn compose(readings: &[WindowReading], today: Today, zone: &TimeZone, now_ms: i64) -> Glance {
    let ordered = in_order(readings, |reading| reading);
    let mut lines: Vec<Line> = ordered
        .iter()
        .map(|reading| Line {
            label: window_name(reading),
            value: value(reading, zone, now_ms),
        })
        .collect();
    lines.push(Line {
        label: "Today".to_owned(),
        value: cost(today),
    });
    let sources = ["claude-code", "copilot-cli"]
        .into_iter()
        .filter_map(|agent| {
            let latest = ordered
                .iter()
                .filter(|reading| reading.agent == agent)
                .max_by_key(|reading| reading.observed_ms)?;
            let observed = Timestamp::from_millisecond(latest.observed_ms).ok()?;
            let now = Timestamp::from_millisecond(now_ms).ok()?;
            let from = match latest.provenance {
                Provenance::Official => "Official, status line",
                Provenance::Recorded => "Recorded, Copilot CLI",
                Provenance::Experimental => "Experimental, usage endpoint",
            };
            Some(format!("{from}, {}", ago(observed, now)))
        })
        .collect();
    let current = |window: &str| {
        ordered
            .iter()
            .find(|reading| reading.window == window && !expired(reading, now_ms))
    };
    let five_hour = current("five_hour");
    let allowance = current("monthly");
    // The figure is the person's own five-hour window; Copilot CLI's allowance, which an employer
    // may pay for, only lends it its level, so that it never passes 70% unseen.
    let figure = five_hour.map(|reading| Figure {
        used: reading.used_percentage,
        level: allowance.map_or(reading.level, |allowance| {
            higher(reading.level, allowance.level)
        }),
    });
    let mut status: Vec<String> = Vec::new();
    if let Some(reading) = five_hour {
        status.push(format!("5h {}", value(reading, zone, now_ms)));
    }
    if let Some(reading) = allowance {
        let mark = if reading.level == Level::Normal {
            ""
        } else {
            " ▲"
        };
        status.push(format!(
            "Copilot {}{mark}",
            percent(reading.used_percentage)
        ));
    }
    let tooltip = if status.is_empty() {
        format!("{NAME} - {} today", cost(today))
    } else {
        format!("{NAME} - {}", status.join(" · "))
    };
    let title = match figure {
        Some(figure) => match figure.level {
            Level::Normal => percent(figure.used),
            Level::Warning | Level::Critical => format!("{} ▲", percent(figure.used)),
        },
        None => money(today.usd),
    };
    Glance {
        figure,
        lines,
        sources,
        tooltip,
        title,
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde_json::Value;

    use super::*;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Example {
        name: String,
        now_ms: i64,
        time_zone: String,
        limits: Vec<WindowReading>,
        today: TodayExample,
        expect: Expect,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TodayExample {
        usd: f64,
        unpriced_requests: u64,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Expect {
        lines: Vec<[String; 2]>,
        sources: Vec<String>,
        tooltip: String,
        title: String,
        icon: Option<f64>,
    }

    #[test]
    fn the_glance_says_what_the_shared_examples_say() {
        let examples: Value =
            serde_json::from_str(include_str!("../../../design/glance.json")).unwrap();
        let examples: Vec<Example> = serde_json::from_value(examples["examples"].clone()).unwrap();
        assert!(!examples.is_empty());
        for example in examples {
            let zone = TimeZone::get(&example.time_zone).unwrap();
            let glance = compose(
                &example.limits,
                Today {
                    usd: example.today.usd,
                    unpriced_requests: example.today.unpriced_requests,
                },
                &zone,
                example.now_ms,
            );
            let lines: Vec<[String; 2]> = glance
                .lines
                .iter()
                .map(|line| [line.label.clone(), line.value.clone()])
                .collect();
            assert_eq!(lines, example.expect.lines, "{}", example.name);
            assert_eq!(glance.sources, example.expect.sources, "{}", example.name);
            assert_eq!(glance.tooltip, example.expect.tooltip, "{}", example.name);
            assert_eq!(glance.title, example.expect.title, "{}", example.name);
            assert_eq!(
                glance.figure.map(|figure| figure.used),
                example.expect.icon,
                "{}",
                example.name
            );
        }
    }

    #[test]
    fn a_menu_line_joins_its_label_and_value() {
        let line = Line {
            label: "Today".to_owned(),
            value: "$5.05".to_owned(),
        };
        assert_eq!(line.text(), "Today: $5.05");
    }
}
