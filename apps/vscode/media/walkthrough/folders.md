## Data folders

agent-dashboard reads the folders where your coding agents keep their transcripts:

- Claude Code's: the one `CLAUDE_CONFIG_DIR` names, or `~/.claude`.
- Copilot CLI's: the one `COPILOT_HOME` names, or `~/.copilot`, which also holds the sessions VS Code runs on its Copilot harness.
- Copilot in VS Code's: VS Code's user data folder, and Insiders' too, unless `AGENT_DASHBOARD_VSCODE_DATA` names another.

Add another when an agent also runs elsewhere on this machine, such as in WSL, and choose which agent's data it holds.
Every surface of agent-dashboard reads the same folders, the command line included.
