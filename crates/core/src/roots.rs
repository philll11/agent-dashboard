//! Finding the agents' data roots and what they contain.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::agent::Agent;
use crate::{copilot, transcript, vscode};

/// The environment variable Claude Code reads to move its configuration folder.
pub const CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";

/// The environment variable Copilot CLI reads to move its data folder.
pub const COPILOT_HOME_ENV: &str = "COPILOT_HOME";

/// The environment variable that names VS Code's user data folder for agent-dashboard alone,
/// ahead of VS Code's own variables, such as for tests.
pub const VSCODE_DATA_ENV: &str = "AGENT_DASHBOARD_VSCODE_DATA";

/// The environment variable a portable VS Code sets to its data folder, whose `user-data` folder
/// holds its user data.
pub const VSCODE_PORTABLE_ENV: &str = "VSCODE_PORTABLE";

/// The environment variable that moves the folder VS Code keeps each product's user data in.
pub const VSCODE_APPDATA_ENV: &str = "VSCODE_APPDATA";

/// VS Code's user data folder under the folder for application data.
const VSCODE: &str = "Code";

/// VS Code Insiders' user data folder under the folder for application data, read when it exists.
const VSCODE_INSIDERS: &str = "Code - Insiders";

/// Where a data root came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSource {
    /// Named by an environment variable, which takes the place of the default: the agent's own,
    /// such as `CLAUDE_CONFIG_DIR`, `COPILOT_HOME` or VS Code's `VSCODE_APPDATA`, or
    /// `AGENT_DASHBOARD_VSCODE_DATA`.
    Environment { variable: &'static str },
    /// The agent's default location: `~/.claude`, `~/.copilot` or VS Code's user data folder.
    Default,
    /// Added by the person, for example a WSL home.
    Added,
}

/// A data root to read, the agent whose data it holds, and where it came from.
///
/// Copilot CLI's data root also holds the sessions that VS Code runs on its Copilot harness,
/// which belong to Copilot in VS Code; VS Code's own data root holds those of its Local harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataRoot {
    pub path: PathBuf,
    pub agent: Agent,
    pub source: RootSource,
}

impl DataRoot {
    /// A data root the person added, holding `agent`'s data.
    #[must_use]
    pub fn added(path: impl Into<PathBuf>, agent: Agent) -> Self {
        Self {
            path: path.into(),
            agent,
            source: RootSource::Added,
        }
    }

    /// The folder in the data root that holds every transcript in it: Claude Code's `projects`,
    /// Copilot CLI's `session-state`, or VS Code's `User`.
    #[must_use]
    pub fn transcripts(&self) -> PathBuf {
        self.path.join(match self.agent {
            Agent::ClaudeCode => "projects",
            Agent::CopilotCli => copilot::SESSIONS,
            Agent::CopilotVscode => vscode::USER,
        })
    }
}

/// What a data root holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootStatus {
    /// The folder exists; `transcripts` and `bytes` cover every transcript of the agent's that
    /// it holds.
    Found { transcripts: u64, bytes: u64 },
    /// The folder does not exist.
    Missing,
    /// The folder is on a network or WSL location that cannot be reached.
    Unreachable { reason: String },
    /// The folder exists but cannot be read.
    Unreadable { reason: String },
}

/// What the agents' data roots are found from: the person's home folder, the platform's folder
/// for application data, and the agents' environment variables, each `None` where it is not set.
#[derive(Debug, Clone, Copy, Default)]
pub struct Environment<'a> {
    pub home: Option<&'a Path>,
    /// The folder VS Code keeps each product's user data in by default: `%APPDATA%` on Windows,
    /// `~/Library/Application Support` on macOS, `$XDG_CONFIG_HOME` or `~/.config` on Linux.
    pub app_data: Option<&'a Path>,
    /// `CLAUDE_CONFIG_DIR`.
    pub claude_config_dir: Option<&'a OsStr>,
    /// `COPILOT_HOME`.
    pub copilot_home: Option<&'a OsStr>,
    /// `AGENT_DASHBOARD_VSCODE_DATA`.
    pub vscode_data: Option<&'a OsStr>,
    /// `VSCODE_PORTABLE`.
    pub vscode_portable: Option<&'a OsStr>,
    /// `VSCODE_APPDATA`.
    pub vscode_appdata: Option<&'a OsStr>,
}

/// Lists the data roots to read, in order and without duplicates: Claude Code's, then Copilot
/// CLI's, then VS Code's, then the ones the person added.
///
/// `CLAUDE_CONFIG_DIR` is one path or a comma-separated list, where a leading `~` stands for the
/// home folder; when it names at least one folder it replaces `~/.claude`, as it does for Claude
/// Code. `COPILOT_HOME` replaces `~/.copilot` in the same way. VS Code's user data folder is the
/// one `AGENT_DASHBOARD_VSCODE_DATA` names, else found as VS Code finds it: the `user-data` folder
/// of a portable VS Code's `VSCODE_PORTABLE`, else `Code` in `VSCODE_APPDATA` or the folder for
/// application data, with Insiders' `Code - Insiders` beside it when that exists. `added` roots,
/// each with its agent, always come last.
#[must_use]
pub fn data_roots(environment: &Environment<'_>, added: &[(PathBuf, Agent)]) -> Vec<DataRoot> {
    let home = environment.home;
    let mut roots: Vec<DataRoot> = Vec::new();
    let mut push = |path: PathBuf, agent: Agent, source: RootSource| {
        if !roots
            .iter()
            .any(|root| root.agent == agent && same_path(&root.path, &path))
        {
            roots.push(DataRoot {
                path,
                agent,
                source,
            });
        }
    };

    let from_env: Vec<PathBuf> = environment
        .claude_config_dir
        .map(|value| split_list(&value.to_string_lossy(), home))
        .unwrap_or_default();
    if from_env.is_empty() {
        if let Some(home) = home {
            push(home.join(".claude"), Agent::ClaudeCode, RootSource::Default);
        }
    } else {
        for path in from_env {
            push(
                path,
                Agent::ClaudeCode,
                RootSource::Environment {
                    variable: CONFIG_DIR_ENV,
                },
            );
        }
    }

    match (set(environment.copilot_home), home) {
        (Some(value), _) => push(
            expand_home(&value, home),
            Agent::CopilotCli,
            RootSource::Environment {
                variable: COPILOT_HOME_ENV,
            },
        ),
        (None, Some(home)) => push(
            home.join(".copilot"),
            Agent::CopilotCli,
            RootSource::Default,
        ),
        (None, None) => {}
    }

    if let Some(value) = set(environment.vscode_data) {
        push(
            expand_home(&value, home),
            Agent::CopilotVscode,
            RootSource::Environment {
                variable: VSCODE_DATA_ENV,
            },
        );
    } else if let Some(value) = set(environment.vscode_portable) {
        push(
            PathBuf::from(value).join("user-data"),
            Agent::CopilotVscode,
            RootSource::Environment {
                variable: VSCODE_PORTABLE_ENV,
            },
        );
    } else {
        let (base, source) = match set(environment.vscode_appdata) {
            Some(value) => (
                Some(PathBuf::from(value)),
                RootSource::Environment {
                    variable: VSCODE_APPDATA_ENV,
                },
            ),
            None => (
                environment.app_data.map(Path::to_path_buf),
                RootSource::Default,
            ),
        };
        if let Some(base) = base {
            push(base.join(VSCODE), Agent::CopilotVscode, source);
            let insiders = base.join(VSCODE_INSIDERS);
            if insiders.is_dir() {
                push(insiders, Agent::CopilotVscode, source);
            }
        }
    }

    for (path, agent) in added {
        push(path.clone(), *agent, RootSource::Added);
    }
    roots
}

/// The folder VS Code keeps each product's user data in by default, found as VS Code finds it:
/// `%APPDATA%` on Windows, else `AppData\Roaming` in the home folder; `~/Library/Application
/// Support` on macOS; `$XDG_CONFIG_HOME` elsewhere, else `~/.config`.
#[must_use]
pub fn app_data(
    home: Option<&Path>,
    appdata: Option<&OsStr>,
    xdg_config_home: Option<&OsStr>,
) -> Option<PathBuf> {
    let named = |value: Option<&OsStr>| value.filter(|value| !value.is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        named(appdata).or_else(|| home.map(|home| home.join("AppData").join("Roaming")))
    } else if cfg!(target_os = "macos") {
        home.map(|home| home.join("Library").join("Application Support"))
    } else {
        named(xdg_config_home).or_else(|| home.map(|home| home.join(".config")))
    }
}

/// An environment variable's value, trimmed; `None` when it is unset or blank.
fn set(value: Option<&OsStr>) -> Option<String> {
    value
        .map(|value| value.to_string_lossy().trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Looks at a data root and counts its agent's transcripts: Claude Code's under its `projects`
/// folder, Copilot's under its `session-state` folder, and VS Code's chat session files in its
/// workspaces' storage.
#[must_use]
pub fn inspect(root: &Path, agent: Agent) -> RootStatus {
    match std::fs::metadata(root) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => {
            return RootStatus::Unreadable {
                reason: "it is a file, not a folder".to_owned(),
            };
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return RootStatus::Missing,
        Err(error) if is_wsl_location(root) => {
            // Windows answers with misleading errors here, such as "The user name or password
            // is incorrect" for a distribution that is not installed.
            return RootStatus::Unreachable {
                reason: format!(
                    "the WSL distribution is not installed or not running ({})",
                    os_code(&error)
                ),
            };
        }
        Err(error) if is_network_location(root) => {
            return RootStatus::Unreachable {
                reason: error.to_string(),
            };
        }
        Err(error) => {
            return RootStatus::Unreadable {
                reason: error.to_string(),
            };
        }
    }

    let (folder, is_transcript): (&str, fn(&Path) -> bool) = match agent {
        Agent::ClaudeCode => ("projects", transcript::is_transcript),
        Agent::CopilotCli => (copilot::SESSIONS, copilot::is_transcript),
        // VS Code's user data folder holds much else, so its chat session files are listed where
        // they are rather than found by walking it.
        Agent::CopilotVscode => {
            let found = vscode::find(root);
            return RootStatus::Found {
                transcripts: found.len() as u64,
                bytes: found.iter().map(|transcript| transcript.len).sum(),
            };
        }
    };
    let projects = root.join(folder);
    if !projects.exists() {
        return RootStatus::Found {
            transcripts: 0,
            bytes: 0,
        };
    }

    let mut transcripts = 0;
    let mut bytes = 0;
    for entry in WalkDir::new(&projects) {
        let entry = match entry {
            Ok(entry) => entry,
            // A folder that cannot be listed at the top means the whole root is unreadable;
            // deeper failures only hide part of it, so counting continues.
            Err(error) if error.depth() == 0 => {
                return RootStatus::Unreadable {
                    reason: error.to_string(),
                };
            }
            Err(_) => continue,
        };
        let found = entry
            .path()
            .strip_prefix(&projects)
            .is_ok_and(is_transcript);
        if entry.file_type().is_file() && found {
            transcripts += 1;
            bytes += entry.metadata().map_or(0, |meta| meta.len());
        }
    }
    RootStatus::Found { transcripts, bytes }
}

fn split_list(value: &str, home: Option<&Path>) -> Vec<PathBuf> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| expand_home(entry, home))
        .collect()
}

fn expand_home(entry: &str, home: Option<&Path>) -> PathBuf {
    let Some(home) = home else {
        return PathBuf::from(entry);
    };
    if entry == "~" {
        return home.to_path_buf();
    }
    match entry
        .strip_prefix("~/")
        .or_else(|| entry.strip_prefix("~\\"))
    {
        Some(rest) => home.join(rest),
        None => PathBuf::from(entry),
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let normalise = |path: &Path| {
        let text = path.to_string_lossy().replace('\\', "/");
        let trimmed = text.trim_end_matches('/').to_owned();
        if cfg!(windows) {
            trimmed.to_lowercase()
        } else {
            trimmed
        }
    };
    normalise(a) == normalise(b)
}

fn is_network_location(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with(r"\\") || text.starts_with("//")
}

fn is_wsl_location(path: &Path) -> bool {
    let text = path.to_string_lossy().replace('/', "\\").to_lowercase();
    text.starts_with(r"\\wsl.localhost\") || text.starts_with(r"\\wsl$\")
}

fn os_code(error: &io::Error) -> String {
    error.raw_os_error().map_or_else(
        || error.kind().to_string(),
        |code| format!("os error {code}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from("/home/ada")
    }

    fn app_data() -> PathBuf {
        PathBuf::from("/home/ada/.config")
    }

    /// The environment with a home folder and its folder for application data, and nothing set.
    fn environment<'a>(home: &'a Path, app_data: &'a Path) -> Environment<'a> {
        Environment {
            home: Some(home),
            app_data: Some(app_data),
            ..Environment::default()
        }
    }

    fn of(roots: &[DataRoot], agent: Agent) -> Vec<&DataRoot> {
        roots.iter().filter(|root| root.agent == agent).collect()
    }

    fn from(variable: &'static str) -> RootSource {
        RootSource::Environment { variable }
    }

    #[test]
    fn defaults_to_dot_claude_dot_copilot_and_vs_code_s_user_data() {
        let (home, app_data) = (home(), app_data());
        let roots = data_roots(&environment(&home, &app_data), &[]);
        assert_eq!(
            roots,
            vec![
                DataRoot {
                    path: home.join(".claude"),
                    agent: Agent::ClaudeCode,
                    source: RootSource::Default
                },
                DataRoot {
                    path: home.join(".copilot"),
                    agent: Agent::CopilotCli,
                    source: RootSource::Default
                },
                DataRoot {
                    path: app_data.join("Code"),
                    agent: Agent::CopilotVscode,
                    source: RootSource::Default
                },
            ]
        );
    }

    #[test]
    fn config_dir_replaces_the_default() {
        let (home, app_data) = (home(), app_data());
        let roots = data_roots(
            &Environment {
                claude_config_dir: Some(OsStr::new("/data/claude")),
                ..environment(&home, &app_data)
            },
            &[],
        );
        assert_eq!(
            of(&roots, Agent::ClaudeCode),
            vec![&DataRoot {
                path: PathBuf::from("/data/claude"),
                agent: Agent::ClaudeCode,
                source: from(CONFIG_DIR_ENV)
            }]
        );
    }

    #[test]
    fn copilot_home_replaces_the_default() {
        let (home, app_data) = (home(), app_data());
        let roots = data_roots(
            &Environment {
                copilot_home: Some(OsStr::new("~/work/copilot")),
                ..environment(&home, &app_data)
            },
            &[],
        );
        assert_eq!(
            of(&roots, Agent::CopilotCli),
            vec![&DataRoot {
                path: home.join("work/copilot"),
                agent: Agent::CopilotCli,
                source: from(COPILOT_HOME_ENV)
            }]
        );
    }

    #[test]
    fn config_dir_accepts_a_comma_separated_list() {
        let (home, app_data) = (home(), app_data());
        let roots = data_roots(
            &Environment {
                claude_config_dir: Some(OsStr::new(" /a , ,~/work/.claude,/b ")),
                ..environment(&home, &app_data)
            },
            &[],
        );
        let roots = of(&roots, Agent::ClaudeCode);
        let paths: Vec<_> = roots.iter().map(|root| root.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/a"),
                home.join("work/.claude"),
                PathBuf::from("/b")
            ]
        );
        assert!(roots.iter().all(|root| root.source == from(CONFIG_DIR_ENV)));
    }

    #[test]
    fn blank_variables_fall_back_to_the_defaults() {
        let (home, app_data) = (home(), app_data());
        let roots = data_roots(
            &Environment {
                claude_config_dir: Some(OsStr::new(" , ")),
                copilot_home: Some(OsStr::new("  ")),
                vscode_data: Some(OsStr::new(" ")),
                vscode_portable: Some(OsStr::new("")),
                vscode_appdata: Some(OsStr::new(" ")),
                ..environment(&home, &app_data)
            },
            &[],
        );
        assert!(roots.iter().all(|root| root.source == RootSource::Default));
        assert_eq!(roots.len(), 3);
    }

    #[test]
    fn vs_code_s_folder_is_found_as_vs_code_finds_it() {
        let (home, app_data) = (home(), app_data());
        let vscode = |environment: Environment<'_>| -> Vec<(PathBuf, RootSource)> {
            of(&data_roots(&environment, &[]), Agent::CopilotVscode)
                .into_iter()
                .map(|root| (root.path.clone(), root.source))
                .collect()
        };
        assert_eq!(
            vscode(Environment {
                vscode_data: Some(OsStr::new("~/vscode-data")),
                vscode_portable: Some(OsStr::new("/opt/code/data")),
                vscode_appdata: Some(OsStr::new("/elsewhere")),
                ..environment(&home, &app_data)
            }),
            [(home.join("vscode-data"), from(VSCODE_DATA_ENV))]
        );
        assert_eq!(
            vscode(Environment {
                vscode_portable: Some(OsStr::new("/opt/code/data")),
                vscode_appdata: Some(OsStr::new("/elsewhere")),
                ..environment(&home, &app_data)
            }),
            [(
                PathBuf::from("/opt/code/data/user-data"),
                from(VSCODE_PORTABLE_ENV)
            )]
        );
        assert_eq!(
            vscode(Environment {
                vscode_appdata: Some(OsStr::new("/elsewhere")),
                ..environment(&home, &app_data)
            }),
            [(PathBuf::from("/elsewhere/Code"), from(VSCODE_APPDATA_ENV))]
        );
    }

    #[test]
    fn vs_code_insiders_is_read_when_it_is_there() {
        let base = tempfile::tempdir().unwrap();
        std::fs::create_dir(base.path().join(VSCODE_INSIDERS)).unwrap();
        let home = home();
        let roots = data_roots(&environment(&home, base.path()), &[]);
        let paths: Vec<_> = of(&roots, Agent::CopilotVscode)
            .into_iter()
            .map(|root| root.path.clone())
            .collect();
        assert_eq!(
            paths,
            [base.path().join(VSCODE), base.path().join(VSCODE_INSIDERS)]
        );
    }

    #[test]
    fn added_roots_come_last_without_duplicates() {
        let (home, app_data) = (home(), app_data());
        let added = vec![
            (PathBuf::from("/mnt/wsl/.claude"), Agent::ClaudeCode),
            (home.join(".claude/"), Agent::ClaudeCode),
            (PathBuf::from("/mnt/wsl/.copilot"), Agent::CopilotCli),
            (app_data.join("Code"), Agent::CopilotVscode),
            (
                PathBuf::from("/portable/data/user-data"),
                Agent::CopilotVscode,
            ),
        ];
        let roots = data_roots(&environment(&home, &app_data), &added);
        let kinds: Vec<_> = roots.iter().map(|root| (root.agent, root.source)).collect();
        assert_eq!(
            kinds,
            vec![
                (Agent::ClaudeCode, RootSource::Default),
                (Agent::CopilotCli, RootSource::Default),
                (Agent::CopilotVscode, RootSource::Default),
                (Agent::ClaudeCode, RootSource::Added),
                (Agent::CopilotCli, RootSource::Added),
                (Agent::CopilotVscode, RootSource::Added),
            ]
        );
    }

    #[test]
    fn no_home_and_no_variables_means_no_default() {
        assert_eq!(data_roots(&Environment::default(), &[]), Vec::new());
    }

    #[test]
    fn counts_vs_code_s_chat_session_files_without_walking_its_folder() {
        let root = tempfile::tempdir().unwrap();
        let sessions = root.path().join("User/workspaceStorage/abc/chatSessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(sessions.join("s1.jsonl"), "{}\n").unwrap();
        std::fs::create_dir_all(root.path().join("User/History/x")).unwrap();
        std::fs::write(root.path().join("User/History/x/s2.jsonl"), "{}\n").unwrap();
        assert_eq!(
            inspect(root.path(), Agent::CopilotVscode),
            RootStatus::Found {
                transcripts: 1,
                bytes: 3
            }
        );
    }

    #[test]
    fn recognises_wsl_and_network_locations() {
        let wsl = Path::new(r"\\wsl.localhost\Ubuntu\home\ada\.claude");
        let legacy_wsl = Path::new(r"\\WSL$\Ubuntu\home\ada\.claude");
        let share = Path::new(r"\\fileserver\home\ada\.claude");
        let local = Path::new(r"C:\Users\ada\.claude");

        assert!(is_wsl_location(wsl) && is_network_location(wsl));
        assert!(is_wsl_location(legacy_wsl));
        assert!(!is_wsl_location(share) && is_network_location(share));
        assert!(!is_wsl_location(local) && !is_network_location(local));
    }
}
