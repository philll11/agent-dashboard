//! Drawing the state, as the design's terminal boards draw each screen: a header with the
//! sections, the section, and a line of keys with when the history was last read.

mod chart;
mod limits;
mod models;
mod overlays;
mod overview;
pub mod projects;
mod series;
mod session;
mod sessions;

use agent_dashboard_core::agent::Agent;
use agent_dashboard_server::fmt::{ago, grouped};
use agent_dashboard_server::protocol::{CostProvenance, Overview, Totals};
use jiff::Timestamp;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::answers::Request;
use super::canvas::{Canvas, fit, width};
use super::state::{ExportNotice, Section, State, agent_name as named_agent};
use super::theme::Series;

#[cfg(test)]
pub use overlays::HELP_KEYS;
pub use series::{agent_series, series_of};

/// The key line's keys on most screens.
pub const KEYS: [(&str, &str); 5] = [
    ("q", "quit"),
    ("←→", "sections"),
    ("t", "table view"),
    ("e", "export"),
    ("?", "help"),
];

/// Draws the whole screen.
pub fn draw(buf: &mut Buffer, state: &State) {
    let mut canvas = Canvas::new(buf, state.theme);
    let c = &mut canvas;
    if state.too_small() {
        too_small(c);
        return;
    }
    let Some(status) = state.status() else {
        match (state.status_error(), state.health()) {
            (_, Some(health)) if health.history.is_none() && health.newer_schema.is_some() => {
                too_new(c, health.newer_schema.unwrap_or_default());
            }
            (Some(error), _) => cannot_open(c, state, error),
            _ => {
                header(c, state, Some(state.section));
                footer(c, state, &KEYS, None);
            }
        }
        return;
    };
    let nothing_yet = status.first_day.is_none()
        && reading(state).is_none()
        && status.limits.is_empty()
        && status.read_ms.is_some();
    if nothing_yet {
        empty(c, state);
    } else {
        match state.section {
            Section::Overview => overview::draw(c, state),
            Section::Sessions if state.session.is_some() => session::draw(c, state),
            Section::Sessions => sessions::draw(c, state),
            Section::Projects => projects::draw(c, state),
            Section::Models => models::draw(c, state),
            Section::Limits => limits::draw(c, state),
        }
    }
    if let Some(export) = &state.export {
        export_notice(c, export);
    }
    if let Some(overlay) = &state.overlay {
        overlays::draw(c, state, overlay);
    }
}

/// How many rows the section's page has beyond the screen, for scrolling it.
pub fn page_overflow(state: &State) -> i32 {
    let page = match state.section {
        Section::Sessions if state.session.is_some() => session::height(state),
        Section::Models => models::height(state),
        Section::Limits => limits::height(state),
        _ => 0,
    };
    page - page_rows(state)
}

/// Where the costs shown come from: `totals`' own provenance, or, while none of their requests is
/// priced, where the agents in view take theirs from.
pub fn provenance(state: &State, totals: Option<&Totals>) -> CostProvenance {
    if let Some(provenance) = totals.and_then(|totals| totals.cost_provenance) {
        return provenance;
    }
    let agents: Vec<Agent> = state
        .agents_in_view()
        .into_iter()
        .filter_map(Agent::from_name)
        .collect();
    match (
        agents.iter().any(|agent| !agent.records_charges()),
        agents.iter().any(|agent| agent.records_charges()),
    ) {
        (true, true) => CostProvenance::Mixed,
        (false, true) => CostProvenance::Recorded,
        _ => CostProvenance::Estimate,
    }
}

/// One agent's part of what the screen shows, while usage splits by agent.
pub struct AgentPart<'a> {
    pub id: &'a str,
    pub name: String,
    /// The series the agent takes in charts, and its mark beside its name.
    pub series: Series,
    pub overview: &'a Overview,
}

/// Each agent's part of `request`, as the server answers it for that agent alone: `None` while
/// usage does not split by agent, and until every agent's part has come.
pub fn agent_parts(state: &State, request: Option<Request>) -> Option<Vec<AgentPart<'_>>> {
    if !state.by_agent() {
        return None;
    }
    Some(
        state
            .agent_overviews(request)?
            .into_iter()
            .map(|(id, overview)| AgentPart {
                id,
                name: agent_name(state, id),
                series: agent_series(id),
                overview,
            })
            .collect(),
    )
}

/// What a cost is called, by where it comes from.
pub fn cost_label(provenance: CostProvenance) -> &'static str {
    match provenance {
        CostProvenance::Estimate => "API-equivalent cost",
        CostProvenance::Recorded => "Cost (recorded)",
        CostProvenance::Mixed => "Cost (includes estimate)",
    }
}

/// The rows a page shows at once: from the third row to the one above the key line.
pub(super) fn page_rows(state: &State) -> i32 {
    i32::from(state.height) - 3
}

/// Draws a page taller than the screen through `draw_page`, which draws it from its first row,
/// and shows the rows it is scrolled to below the header.
fn scrolled(c: &mut Canvas, state: &State, height: i32, draw_page: impl FnOnce(&mut Canvas)) {
    let rows = page_rows(state);
    let height = height.max(rows);
    let area = Rect::new(0, 0, state.width, u16::try_from(height).unwrap_or(u16::MAX));
    let mut page = Buffer::empty(area);
    draw_page(&mut Canvas::new(&mut page, state.theme));
    let top = state.scroll.clamp(0, height - rows);
    c.copy_rows(&page, top, 2, rows);
}

/// How far a read of new transcripts has got, while one is under way: as the latest notification
/// says, or, before one comes, as the status does.
pub fn reading(state: &State) -> Option<(u64, u64)> {
    state
        .progress
        .or_else(|| state.status().and_then(|status| status.reading))
        .filter(|progress| progress.done < progress.total)
        .map(|progress| (progress.done, progress.total))
}

/// The header: the product, the sections with the selected one marked, and in the wide layout
/// the range and filters.
pub fn header(c: &mut Canvas, state: &State, selected: Option<Section>) {
    let theme = c.theme;
    let filters = filters(state);
    // The filters keep two columns apart from the sections.
    let filters_room = if state.wide() { width(&filters) + 2 } else { 0 };
    let mut x = c.text(1, 0, brand(c.w - filters_room), theme.strong()) + 3;
    for section in Section::ALL {
        let style = if Some(section) == selected {
            theme.selected()
        } else {
            theme.muted()
        };
        x = c.text(x, 0, &format!(" {} ", section.name()), style) + 1;
    }
    if state.wide() {
        c.right(c.w - 1, 0, &fit(&filters, c.w - 1 - (x + 1)), theme.muted());
    }
}

/// The product's name for a header `columns` wide: in full where it leaves room for the sections
/// beside it, and otherwise its first word, as the desktop app's sidebar sets it apart.
fn brand(columns: i32) -> &'static str {
    const FULL: &str = "Dashboard for Coding Agents";
    let sections: i32 = Section::ALL
        .iter()
        .map(|section| width(section.name()) + 3)
        .sum();
    if 1 + width(FULL) + 3 + sections <= columns {
        FULL
    } else {
        "Dashboard"
    }
}

/// The range and filters, as the header names them: `30 days · all projects · all models`, with
/// the agent after the range once there is more than one to choose from.
fn filters(state: &State) -> String {
    let project = state
        .project
        .as_ref()
        .map_or_else(|| "all projects".to_owned(), |id| project_name(state, id));
    let model = state
        .model
        .as_ref()
        .map_or_else(|| "all models".to_owned(), |id| model_name(state, id));
    if state.agent_shown() {
        let agent = state
            .agent
            .as_ref()
            .map_or_else(|| "all agents".to_owned(), |id| agent_name(state, id));
        format!("{} · {agent} · {project} · {model}", state.range.title())
    } else {
        format!("{} · {project} · {model}", state.range.title())
    }
}

/// The filters chosen, for the narrow layouts, whose header has no room for them: `None` when
/// none is.
pub fn chosen_filters(state: &State) -> Option<String> {
    let parts: Vec<String> = [
        state.agent.as_ref().map(|id| agent_name(state, id)),
        state.project.as_ref().map(|id| project_name(state, id)),
        state.model.as_ref().map(|id| model_name(state, id)),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty() && !state.wide()).then(|| parts.join(" · "))
}

/// An agent's name, as the history's choices give it: `Copilot CLI`.
pub fn agent_name(state: &State, id: &str) -> String {
    super::canvas::clean(
        &state
            .choices()
            .and_then(|choices| choices.agents.iter().find(|agent| agent.id == id))
            .map_or_else(|| named_agent(id), |agent| agent.name.clone()),
    )
}

pub fn project_name(state: &State, id: &str) -> String {
    super::canvas::clean(
        &state
            .choices()
            .and_then(|choices| choices.projects.iter().find(|project| project.id == id))
            .map_or_else(|| id.to_owned(), |project| project.name.clone()),
    )
}

pub fn model_name(state: &State, id: &str) -> String {
    super::canvas::clean(
        &state
            .choices()
            .and_then(|choices| choices.models.iter().find(|model| model.id == id))
            .map_or_else(|| id.to_owned(), |model| model.name.clone()),
    )
}

/// The key line: keys at the left, as many as fit before the note at the right, which says when
/// the history was last read unless `note` says otherwise.
pub fn footer(c: &mut Canvas, state: &State, keys: &[(&str, &str)], note: Option<&str>) {
    let theme = c.theme;
    let y = c.h - 1;
    let (shown, style) = if let Some(note) = note {
        (note.to_owned(), theme.muted())
    } else {
        let (read, style) = read_note(state);
        let shown = if state.wide() && !read.is_empty() {
            let costs = match provenance(state, None) {
                CostProvenance::Estimate => "costs are API-equivalent estimates",
                CostProvenance::Recorded => "costs are as Copilot recorded them",
                CostProvenance::Mixed => "costs include API-equivalent estimates",
            };
            format!("{read} · {costs}")
        } else {
            read
        };
        (shown, style)
    };
    let note_start = c.right(c.w - 1, y, &shown, style);
    // The keys close up to two spaces apart before any is left out.
    let all: i32 = keys
        .iter()
        .map(|(key, label)| width(key) + 1 + width(label))
        .sum::<i32>()
        + 3 * (i32::try_from(keys.len()).unwrap_or(1) - 1);
    let gap = if 1 + all > note_start - 2 { 2 } else { 3 };
    let mut x = 1;
    for (key, label) in keys {
        if x + width(key) + 1 + width(label) > note_start - 2 {
            continue;
        }
        x = c.text(x, y, key, theme.key());
        x = c.text(x + 1, y, label, theme.muted()) + gap;
    }
}

/// Where the last export stands, on the line under the header: under way, saved with as much of
/// its path as fits, keeping its end, or why it failed.
fn export_notice(c: &mut Canvas, export: &ExportNotice) {
    let theme = c.theme;
    c.clear(0, 1, c.w, 1);
    match export {
        ExportNotice::Running(what) => {
            c.text(1, 1, &fit(&format!("{what}…"), c.w - 2), theme.muted());
        }
        ExportNotice::Saved { what, path } => {
            let x = c.text(1, 1, "✓ ", theme.good());
            let x = c.text(x, 1, &format!("Saved {what} to "), theme.text());
            c.text(
                x,
                1,
                &tail(&super::canvas::clean(path), c.w - 1 - x),
                theme.muted(),
            );
        }
        ExportNotice::Failed(reason) => {
            let x = c.text(1, 1, "▲ ", theme.warn());
            c.text(
                x,
                1,
                &fit(
                    &format!("Cannot export: {}", super::canvas::clean(reason)),
                    c.w - 1 - x,
                ),
                theme.text(),
            );
        }
    }
}

/// As much of `text` as fits in `cells`, keeping its end: `…rd-sessions-2026-10-10.csv`.
pub fn tail(text: &str, cells: i32) -> String {
    if width(text) <= cells {
        return text.to_owned();
    }
    let mut kept: Vec<char> = Vec::new();
    let mut used = 1;
    for c in text.chars().rev() {
        let cell = width(&c.to_string());
        if used + cell > cells {
            break;
        }
        used += cell;
        kept.push(c);
    }
    kept.reverse();
    format!("…{}", kept.into_iter().collect::<String>())
}

/// When the history was last read, or how far the read under way has got.
fn read_note(state: &State) -> (String, Style) {
    let theme = state.theme;
    if let Some((done, total)) = reading(state) {
        return (
            format!(
                "Reading history · {} of {} files",
                grouped(done),
                grouped(total)
            ),
            theme.text(),
        );
    }
    match state.status().and_then(|status| status.read_ms) {
        Some(read_ms) => (
            format!(
                "Read {}",
                Timestamp::from_millisecond(read_ms)
                    .map_or_else(|_| "just now".to_owned(), |read| ago(read, state.now))
            ),
            theme.muted(),
        ),
        // The server has said a read ended, and the status has not caught up with it yet.
        None if state.progress.is_some() => ("Read just now".to_owned(), theme.muted()),
        None if state.status().is_some() => ("Reading history".to_owned(), theme.text()),
        None => (String::new(), theme.muted()),
    }
}

/// A line under the header that the section needs to say first, with a warning mark when it is
/// a warning; in the narrow layouts the chosen filters share the line, at its right.
pub fn notice(c: &mut Canvas, state: &State, text: &str, warning: bool) {
    let theme = c.theme;
    let mut end = c.w - 1;
    if let Some(filters) = chosen_filters(state) {
        end = c.right(c.w - 1, 1, &fit(&filters, 40), theme.muted()) - 3;
    }
    let mut x = 1;
    if warning {
        x = c.text(x, 1, "▲ ", theme.warn());
    }
    if !text.is_empty() {
        c.text(
            x,
            1,
            &fit(text, end - x),
            if warning { theme.text() } else { theme.muted() },
        );
    }
}

/// Lines in the middle of the screen, each centred, and a key line with nothing at its right.
fn message(c: &mut Canvas, lines: &[(&str, Style)], keys: Option<&[(&str, &str)]>, state: &State) {
    let count = i32::try_from(lines.len()).unwrap_or(0);
    let top = ((c.h - count) / 2 - 1).max(2);
    for (index, (text, style)) in (0..).zip(lines) {
        if text.is_empty() {
            continue;
        }
        let text = fit(text, c.w - 2);
        let x = ((c.w - width(&text)) / 2).max(1);
        c.text(x, top + index, &text, *style);
    }
    if let Some(keys) = keys {
        footer(c, state, keys, Some(" "));
    }
}

fn too_small(c: &mut Canvas) {
    let theme = c.theme;
    let size = format!("The window is {} by {}.", c.w, c.h);
    let lines = [
        (size.as_str(), theme.strong()),
        ("The dashboard needs at least 80 by 24.", theme.text()),
        ("", theme.text()),
        ("q quits", theme.muted()),
    ];
    let count = i32::try_from(lines.len()).unwrap_or(0);
    let top = ((c.h - count) / 2 - 1).max(0);
    for (index, (text, style)) in (0..).zip(lines) {
        let text = fit(text, c.w - 2);
        let x = ((c.w - width(&text)) / 2).max(1);
        c.text(x, top + index, &text, style);
    }
}

/// No history found, with the folders that were checked.
fn empty(c: &mut Canvas, state: &State) {
    let theme = c.theme;
    header(c, state, Some(Section::Overview));
    let folders: Vec<String> = state.health().map_or_else(Vec::new, |health| {
        health
            .folders
            .iter()
            .map(|folder| super::canvas::clean(&folder.path))
            .collect()
    });
    let looked = match folders.as_slice() {
        [] => String::new(),
        [one] => format!("We looked in {one}."),
        [rest @ .., last] => format!("We looked in {} and {last}.", rest.join(", ")),
    };
    let mut agents: Vec<&str> = state.health().map_or_else(Vec::new, |health| {
        health
            .folders
            .iter()
            .map(|folder| folder.agent.as_str())
            .collect()
    });
    agents.sort_unstable();
    agents.dedup();
    let (saves, elsewhere) = match agents.as_slice() {
        ["copilot-cli"] => (
            "Copilot CLI saves its history there after your first session.",
            "If you keep it somewhere else, set COPILOT_HOME to that folder.",
        ),
        [] | ["claude-code"] => (
            "Claude Code saves its history there after your first session.",
            "If you keep it somewhere else, set CLAUDE_CONFIG_DIR to that folder.",
        ),
        _ => (
            "Each agent saves its history there after your first session.",
            "To read another folder, set CLAUDE_CONFIG_DIR or COPILOT_HOME to it.",
        ),
    };
    let looked = wrapped(&looked, c.w - 2);
    let mut lines = vec![("No history found", theme.strong()), ("", theme.text())];
    lines.extend(looked.iter().map(|line| (line.as_str(), theme.text())));
    lines.extend([(saves, theme.text()), (elsewhere, theme.text())]);
    message(
        c,
        &lines,
        Some(&[("q", "quit"), ("R", "check again"), ("?", "help")]),
        state,
    );
}

/// `text` in lines of at most `room` columns, broken between words.
fn wrapped(text: &str, room: i32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split(' ') {
        match lines.last_mut() {
            Some(line) if width(line) + 1 + width(word) <= room => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

/// A history that a newer agent-dashboard upgraded beyond what this one reads: nothing else
/// shows.
fn too_new(c: &mut Canvas, schema: u32) {
    let theme = c.theme;
    c.text(1, 0, brand(c.w), theme.strong());
    let upgraded =
        format!("A newer version upgraded it (schema {schema}), and this one cannot read it.");
    let lines = [
        (
            "This history is newer than this agent-dashboard",
            theme.strong(),
        ),
        ("", theme.text()),
        (upgraded.as_str(), theme.text()),
        ("Update agent-dashboard to see it:", theme.text()),
        (
            "github.com/philll11/agent-dashboard/releases/latest",
            theme.accent(),
        ),
    ];
    let count = i32::try_from(lines.len()).unwrap_or(0);
    let top = ((c.h - count) / 2 - 1).max(2);
    for (index, (text, style)) in (0..).zip(lines) {
        if text.is_empty() {
            continue;
        }
        let text = fit(text, c.w - 2);
        let x = ((c.w - width(&text)) / 2).max(1);
        c.text(x, top + index, &text, style);
    }
    let y = c.h - 1;
    let x = c.text(1, y, "q", theme.key());
    c.text(x + 1, y, "quit", theme.muted());
}

/// The history cannot be opened: why, and where to find out more.
fn cannot_open(c: &mut Canvas, state: &State, error: &str) {
    let theme = c.theme;
    header(c, state, Some(Section::Overview));
    let error = super::canvas::clean(error);
    let mut lines: Vec<(String, Style)> = vec![
        ("The history cannot be opened".to_owned(), theme.strong()),
        (String::new(), theme.text()),
    ];
    lines.extend(
        wrap(&sentence(&error), c.w - 8)
            .into_iter()
            .map(|line| (line, theme.text())),
    );
    lines.push((String::new(), theme.text()));
    lines.push((
        "agent-dashboard doctor checks what it can find.".to_owned(),
        theme.muted(),
    ));
    let lines: Vec<(&str, Style)> = lines
        .iter()
        .map(|(text, style)| (text.as_str(), *style))
        .collect();
    message(
        c,
        &lines,
        Some(&[("q", "quit"), ("R", "try again"), ("?", "help")]),
        state,
    );
}

/// A message as a sentence: its first letter capital, and a full stop at its end.
fn sentence(text: &str) -> String {
    let mut chars = text.trim().chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out: String = first.to_uppercase().chain(chars).collect();
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

/// Words in lines of at most `cells`, breaking a line after a colon where one falls.
fn wrap(text: &str, cells: i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if width(&candidate) > cells && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            word.clone_into(&mut line);
        } else {
            line = candidate;
        }
        if line.ends_with(':') {
            lines.push(std::mem::take(&mut line));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
