## Data folders

agent-dashboard reads the folders where your coding agents keep their transcripts:

- Claude Code's: the one `CLAUDE_CONFIG_DIR` names, or `~/.claude`.
- Copilot CLI's: the one `COPILOT_HOME` names, or `~/.copilot`.

Add another when an agent also runs elsewhere on this machine, such as in WSL, and choose which agent's data it holds.
Every surface of agent-dashboard reads the same folders, the command line included.
