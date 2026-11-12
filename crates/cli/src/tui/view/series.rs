//! Which series each model belongs to, as every surface colours them (ADR 0007): each family's
//! newest version in the history gets the family's colour, and every other model shares the series
//! of other models. Copilot names Claude's models with dots, as in `claude-opus-5.5`, so the same
//! version under Claude Code's and Copilot's ids shares the family's colour. While usage splits by
//! agent, each agent takes a series of its own.

use std::collections::HashMap;

use agent_dashboard_server::protocol::ModelRow;

use crate::tui::theme::Series;

/// A model id's family and version, such as `claude-opus-5-5` or `claude-opus-5.5` for Opus 5.5,
/// or `gpt-5.4-nano` for GPT 5.4.
fn parse(model: &str) -> Option<(Series, (u32, u32))> {
    for (prefix, series, anywhere, separators) in [
        ("claude-fable-", Series::Fable, true, &['-', '.'][..]),
        ("claude-opus-", Series::Opus, true, &['-', '.'][..]),
        ("claude-sonnet-", Series::Sonnet, true, &['-', '.'][..]),
        ("claude-haiku-", Series::Haiku, true, &['-', '.'][..]),
        ("gpt-", Series::Gpt, false, &['.'][..]),
        ("gemini-", Series::Gemini, false, &['.'][..]),
    ] {
        let at = if anywhere {
            model.find(prefix)
        } else {
            model.starts_with(prefix).then_some(0)
        };
        let Some(at) = at else {
            continue;
        };
        let rest = &model[at + prefix.len()..];
        let major: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let Ok(major_number) = major.parse::<u32>() else {
            continue;
        };
        let after = &rest[major.len()..];
        let minor = separators
            .iter()
            .find_map(|separator| after.strip_prefix(*separator))
            .map(|tail| {
                tail.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
            })
            .filter(|digits| (1..=2).contains(&digits.len()))
            .and_then(|digits| digits.parse::<u32>().ok())
            .unwrap_or(0);
        return Some((series, (major_number, minor)));
    }
    None
}

/// The series of each of `models`.
pub fn series_of<'a>(models: impl IntoIterator<Item = &'a str>) -> HashMap<String, Series> {
    let models: Vec<&str> = models.into_iter().collect();
    let mut newest: HashMap<Series, (u32, u32)> = HashMap::new();
    for model in &models {
        let Some((series, version)) = parse(model) else {
            continue;
        };
        if newest.get(&series).is_none_or(|best| version > *best) {
            newest.insert(series, version);
        }
    }
    models
        .iter()
        .map(|model| {
            let series = parse(model)
                .filter(|(series, version)| newest.get(series) == Some(version))
                .map_or(Series::Other, |(series, _)| series);
            ((*model).to_owned(), series)
        })
        .collect()
}

/// The series an agent's part of a chart takes while usage splits by agent: Claude Code the first
/// palette slot, Copilot CLI the second and Copilot in VS Code the third, in every theme, so that
/// no two agents share one.
pub fn agent_series(agent: &str) -> Series {
    match agent {
        "claude-code" => Series::Fable,
        "copilot-cli" => Series::Opus,
        "copilot-vscode" => Series::Sonnet,
        _ => Series::Other,
    }
}

/// What each series is called in a legend: its model's name, or `Other models` for several.
#[derive(Debug, Default)]
pub struct SeriesNames(HashMap<Series, String>);

impl SeriesNames {
    /// Series named as given, such as each agent's.
    pub fn named(names: impl IntoIterator<Item = (Series, String)>) -> Self {
        Self(names.into_iter().collect())
    }

    pub fn of(rows: &[ModelRow], series: &HashMap<String, Series>) -> Self {
        let mut names: HashMap<Series, String> = HashMap::new();
        for row in rows {
            let of = series.get(&row.model.id).copied().unwrap_or(Series::Other);
            let own = crate::tui::canvas::clean(&row.model.name);
            names
                .entry(of)
                // One family's newest version under each agent's id keeps its name.
                .and_modify(|name| {
                    if of == Series::Other || *name != own {
                        "Other models".clone_into(name);
                    }
                })
                .or_insert(own);
        }
        Self(names)
    }

    pub fn get(&self, series: Series) -> &str {
        self.0.get(&series).map_or("Other models", String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_familys_newest_model_takes_its_colour() {
        let series = series_of([
            "claude-opus-5-5",
            "claude-opus-5",
            "claude-sonnet-4-5-20250929",
            "claude-opus-4-20250514",
            "glm-4.6",
        ]);
        assert_eq!(series["claude-opus-5-5"], Series::Opus);
        assert_eq!(series["claude-opus-5"], Series::Other);
        assert_eq!(series["claude-opus-4-20250514"], Series::Other);
        assert_eq!(series["claude-sonnet-4-5-20250929"], Series::Sonnet);
        assert_eq!(series["glm-4.6"], Series::Other);
    }

    #[test]
    fn each_agent_takes_a_palette_slot_of_its_own() {
        assert_eq!(agent_series("claude-code"), Series::Fable);
        assert_eq!(agent_series("copilot-cli"), Series::Opus);
        assert_eq!(agent_series("copilot-vscode"), Series::Sonnet);
        assert_eq!(agent_series("codex"), Series::Other);
    }

    #[test]
    fn the_same_version_under_each_agents_id_shares_its_familys_colour() {
        let series = series_of([
            "claude-opus-5-5",
            "claude-opus-5.5",
            "claude-opus-5",
            "gpt-6-luna",
            "gpt-5.4-nano",
            "gpt-4o-mini",
            "gemini-3.1-pro",
            "gemini-2.5-flash",
        ]);
        assert_eq!(series["claude-opus-5-5"], Series::Opus);
        assert_eq!(series["claude-opus-5.5"], Series::Opus);
        assert_eq!(series["claude-opus-5"], Series::Other);
        assert_eq!(series["gpt-6-luna"], Series::Gpt);
        assert_eq!(series["gpt-5.4-nano"], Series::Other);
        assert_eq!(series["gpt-4o-mini"], Series::Other);
        assert_eq!(series["gemini-3.1-pro"], Series::Gemini);
        assert_eq!(series["gemini-2.5-flash"], Series::Other);
    }
}
