//! What the reports and the dashboard call projects and models.

use std::collections::HashMap;

use agent_dashboard_core::history::{History, ProjectKey};

use crate::app::App;

/// What reports call a project: the last part of its working folder, with as many parts before it
/// as tell it apart from every other project's, or else its transcript folder.
pub fn project_names(history: &History) -> HashMap<ProjectKey, (String, Option<String>, String)> {
    let projects = history.projects().unwrap_or_default();
    let names = short_names(
        &projects
            .iter()
            .map(|project| project.path.as_deref())
            .collect::<Vec<_>>(),
    );
    projects
        .into_iter()
        .zip(names)
        .map(|(project, name)| {
            let name = name.unwrap_or_else(|| project.transcript_folder.clone());
            (project.key, (name, project.path, project.transcript_folder))
        })
        .collect()
}

/// Each folder's last part, with as many parts before it as tell it apart from the others.
fn short_names(paths: &[Option<&str>]) -> Vec<Option<String>> {
    let parts: Vec<Vec<&str>> = paths
        .iter()
        .map(|path| {
            path.map_or_else(Vec::new, |path| {
                path.trim_end_matches(['/', '\\'])
                    .rsplit(['/', '\\'])
                    .collect()
            })
        })
        .collect();
    parts
        .iter()
        .enumerate()
        .map(|(index, own)| {
            if own.is_empty() {
                return None;
            }
            let mut count = 1;
            while count < own.len()
                && parts.iter().enumerate().any(|(other, theirs)| {
                    other != index && theirs.get(..count) == own.get(..count)
                })
            {
                count += 1;
            }
            let mut name: Vec<&str> = own[..count].to_vec();
            name.reverse();
            Some(name.join("/"))
        })
        .collect()
}

/// A model's short name, such as `Opus 5.5`: the price table's, or else the one its agent
/// records, such as Copilot CLI's `GPT-6 Luna`, or else its id.
///
/// Copilot CLI's id for a model that Claude Code also uses, such as `claude-opus-5.5` beside
/// `claude-opus-5-5`, is named with its agent, `Opus 5.5, Copilot CLI`, so that the two never
/// share a name.
#[must_use]
pub fn model_name(app: &App, model: &str) -> String {
    model_name_from(
        |id| app.prices.model_name(id),
        app.model_names.get(model).map(String::as_str),
        model,
    )
}

fn model_name_from<'a>(
    priced: impl Fn(&str) -> Option<&'a str>,
    recorded: Option<&str>,
    model: &str,
) -> String {
    let short = |name: &str| name.strip_prefix("Claude ").unwrap_or(name).to_owned();
    if let Some(name) = priced(model) {
        return short(name);
    }
    let Some(recorded) = recorded else {
        return model.to_owned();
    };
    let short = short(recorded);
    if priced(&model.replace('.', "-")).is_some() {
        format!("{short}, Copilot CLI")
    } else {
        short
    }
}

#[cfg(test)]
mod tests {
    use super::{model_name_from, short_names};

    fn priced(id: &str) -> Option<&'static str> {
        (id == "claude-opus-5-5").then_some("Claude Opus 5.5")
    }

    #[test]
    fn a_model_is_named_by_the_price_table_then_by_its_agent() {
        assert_eq!(model_name_from(priced, None, "claude-opus-5-5"), "Opus 5.5");
        assert_eq!(
            model_name_from(priced, Some("GPT-5.5"), "gpt-5.5"),
            "GPT-5.5"
        );
        assert_eq!(model_name_from(priced, None, "glm-4.6"), "glm-4.6");
    }

    #[test]
    fn copilots_id_for_a_claude_code_model_is_named_with_its_agent() {
        assert_eq!(
            model_name_from(priced, Some("Claude Opus 5.5"), "claude-opus-5.5"),
            "Opus 5.5, Copilot CLI"
        );
        assert_eq!(
            model_name_from(priced, Some("Claude Opus 6"), "claude-opus-6"),
            "Opus 6"
        );
    }

    #[test]
    fn a_project_is_named_by_as_much_of_its_folder_as_tells_it_apart() {
        let names = short_names(&[
            Some(r"C:\code\agent-dashboard"),
            Some(r"C:\code\api\workspace"),
            Some(r"C:\bench\matrix\A-fathom-1\workspace"),
            Some(r"C:\bench\smoke\A-fathom-1\workspace\"),
            Some("/home/you/notes"),
            None,
        ]);
        assert_eq!(
            names,
            [
                Some("agent-dashboard".to_owned()),
                Some("api/workspace".to_owned()),
                Some("matrix/A-fathom-1/workspace".to_owned()),
                Some("smoke/A-fathom-1/workspace".to_owned()),
                Some("notes".to_owned()),
                None,
            ]
        );
    }

    #[test]
    fn the_same_folder_twice_keeps_its_whole_path() {
        let names = short_names(&[Some(r"C:\code\app"), Some(r"C:\code\app")]);
        assert_eq!(
            names,
            [
                Some("C:/code/app".to_owned()),
                Some("C:/code/app".to_owned())
            ]
        );
    }
}
