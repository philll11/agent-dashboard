//! The coding agents whose usage the history holds.

use std::fmt;

/// A coding agent whose usage the history holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Agent {
    ClaudeCode,
    /// GitHub Copilot CLI, and the GitHub Copilot app, which keeps its sessions in the same place.
    CopilotCli,
    /// GitHub Copilot in VS Code's Chat view, wherever VS Code keeps the chat: with Copilot CLI's
    /// sessions for its Copilot harness, or in its own files for its Local harness.
    CopilotVscode,
}

/// The plan whose limits an agent's usage counts against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Plan {
    Claude,
    /// A Copilot plan, whose monthly allowance counts Copilot wherever the person uses it.
    Copilot,
}

impl Plan {
    /// Every plan, Claude's first.
    pub const ALL: [Self; 2] = [Self::Claude, Self::Copilot];

    /// The name that the plan's windows carry for every surface.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Copilot => "copilot",
        }
    }

    /// The plan named `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|plan| plan.as_str() == name)
    }

    /// The plan's name as people read it.
    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Copilot => "Copilot",
        }
    }
}

impl Agent {
    /// Every agent, in the order the dashboard lists them.
    pub const ALL: [Self; 3] = [Self::ClaudeCode, Self::CopilotCli, Self::CopilotVscode];

    /// The name that filters and exports use. The history stores Copilot in VS Code's sessions
    /// under Copilot CLI's name, with the harness beside it ([`VscodeHarness`]).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::CopilotCli => "copilot-cli",
            Self::CopilotVscode => "copilot-vscode",
        }
    }

    /// The agent named `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|agent| agent.as_str() == name)
    }

    /// The agent's name as people read it.
    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::CopilotCli => "Copilot CLI",
            Self::CopilotVscode => "Copilot in VS Code",
        }
    }

    /// The agent's name where a column has room for a word, such as a table of sessions.
    #[must_use]
    pub fn short_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude",
            Self::CopilotCli => "Copilot",
            Self::CopilotVscode => "VS Code",
        }
    }

    /// The plan the agent's usage counts against.
    #[must_use]
    pub fn plan(self) -> Plan {
        match self {
            Self::ClaudeCode => Plan::Claude,
            Self::CopilotCli | Self::CopilotVscode => Plan::Copilot,
        }
    }

    /// Whether the agent records what each request cost, so that its cost is never priced from
    /// the price table (ADR 0015).
    #[must_use]
    pub fn records_charges(self) -> bool {
        self.plan() == Plan::Copilot
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// Which of VS Code's harnesses kept a session that VS Code started (ADR 0017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VscodeHarness {
    /// The Copilot harness, which keeps its sessions with Copilot CLI's.
    Copilot,
    /// The Local harness, whose sessions VS Code keeps in its own chat session files, with usage
    /// per prompt.
    Local,
}

impl VscodeHarness {
    /// The name the history stores the harness under.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Copilot => "copilot",
            Self::Local => "local",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Agent, Plan};

    #[test]
    fn every_agent_reads_back_from_its_name() {
        for agent in Agent::ALL {
            assert_eq!(Agent::from_name(agent.as_str()), Some(agent));
        }
        assert_eq!(Agent::from_name("codex"), None);
    }

    #[test]
    fn both_copilot_agents_share_a_plan_and_record_charges() {
        assert_eq!(Agent::CopilotCli.plan(), Plan::Copilot);
        assert_eq!(Agent::CopilotVscode.plan(), Plan::Copilot);
        assert!(Agent::CopilotVscode.records_charges());
        assert!(!Agent::ClaudeCode.records_charges());
    }
}
