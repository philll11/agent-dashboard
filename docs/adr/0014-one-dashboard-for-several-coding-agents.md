# One dashboard for several coding agents

The dashboard shows the usage of several coding agents, so it is called agent-dashboard everywhere: "Dashboard for Coding Agents" where people read it, and `agent-dashboard`, with the short name `agd`, in the repository, the programs, the desktop app's and the extension's identifiers, the settings, the environment variables and the folder that holds the history.
The agent is a dimension of the one history, as project and model are, so every section, filter and export serves every agent, and a person who uses two agents installs one dashboard and sees one status bar item.
ccusage took the same path: it began with a package for each agent and became one tool for all of them.

Before version 2 the product was called claude-dashboard, and people who used it switch over once on each machine.
The first time agent-dashboard opens its own history, it starts from a copy of claude-dashboard's, so no request recorded there is lost; claude-dashboard's file is only ever read.
A Claude Code status line that still runs claude-dashboard is recognised as such, and connecting agent-dashboard's replaces it, keeps the person's own command, and brings in the readings claude-dashboard recorded meanwhile.
Desktop apps before version 2 look for their updates in `latest.json`, which releases no longer publish, so they never replace themselves with an app of another name; the README says how to install the new app and remove the old one.

## Considered options

- **A second product for Copilot CLI** would keep each product's words specific to its agent, but doubles every package, installer and release, and someone who uses both agents runs two of everything.
- **Keeping the name claude-dashboard** would leave every command, folder and identifier naming one of the agents.
- **Renaming only what people read**, keeping the identifiers, would avoid the switch-over but leave the old name in every command and folder for good.
- **Letting older desktop apps update themselves** would install the new app beside the old one on Windows, and on macOS and Linux replace the old one in place under its old name.

## Consequences

- Version 2 is a major version: the programs, environment variables, settings keys and export file names change.
- The history keeps the SQLite application id claude-dashboard gave it, so the adopted copy opens as agent-dashboard's own.
- The desktop app keeps its update signing key.
