//! The coding agents whose usage the history holds.

use std::fmt;

/// A coding agent whose usage the history holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Agent {
    ClaudeCode,
    /// GitHub Copilot CLI, and the GitHub Copilot app, which keeps its sessions in the same place.
    CopilotCli,
}

impl Agent {
    /// Every agent, in the order the dashboard lists them.
    pub const ALL: [Self; 2] = [Self::ClaudeCode, Self::CopilotCli];

    /// The name the history stores the agent under, and that filters and exports use.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::CopilotCli => "copilot-cli",
        }
    }

    /// The agent the history stores under `name`.
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
        }
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::Agent;

    #[test]
    fn every_agent_reads_back_from_its_name() {
        for agent in Agent::ALL {
            assert_eq!(Agent::from_name(agent.as_str()), Some(agent));
        }
        assert_eq!(Agent::from_name("codex"), None);
    }
}
