# Dashboard for Coding Agents

See how you use Claude Code, GitHub Copilot CLI and GitHub Copilot in VS Code: how close you are to your plan limits, what your usage costs, and where it goes.

> New versions are on the [releases page](https://github.com/philll11/agent-dashboard/releases): install the package for your platform with **Extensions: Install from VSIX…**.

## What you get

- **A status bar item** with Claude Code's five-hour window and when it resets, and Copilot's allowance, or today's cost before plan limits are recorded.
  Its tooltip lists every window; selecting it opens the dashboard.
- **A summary in the sidebar**: plan limits, today by model, this week, and this week's top projects.
  Each item opens the matching section of the dashboard.
- **The dashboard in the panel**, in a **Usage dashboard** tab beside the terminal, for keeping your usage in sight while you work.
  A strip at its top shows Claude Code's five-hour and weekly windows, Copilot's allowance and the latest session in the project you have open, whether it is running, and what it has used, whichever section is below.
  Drag the tab beside the terminal or into a sidebar, and it fits itself to the room.
- **The dashboard in an editor tab**: Overview, Sessions with each session's detail, Projects, Models, Limits and Settings.
  It follows your colour theme, high contrast included, and reopens where you left it, as the panel's does.
- **A walkthrough** to record your plan limits, bring in older history and choose data folders.

## How it works

The extension runs `agent-dashboard`, a small program that comes with it for your platform, which reads the transcripts of Claude Code, Copilot CLI and Copilot in VS Code into a history of its own.
Claude Code deletes its transcripts after 30 days by default; the history keeps every request it has read.
The program does the reading in the background, so VS Code's extension host stays idle.

The extension runs where your agents do: in a window connected to WSL, over SSH or in a container, it reads that machine's transcripts.

Copilot in VS Code's sessions are read whichever harness ran them.
The Copilot harness keeps them with Copilot CLI's in `~/.copilot`; the Local harness keeps them in VS Code's own chat session files, which record a prompt's output tokens and charge but not its input or cache tokens, so those show as not recorded.
Inline chat and code completions leave no usage on disk, so they are not counted.

Every surface of agent-dashboard shares the same history: this extension, the command line and the desktop app.

## Plan limits

Copilot notes your monthly allowance in its session files with each request it makes, so it needs nothing connected.
The allowance counts Copilot wherever you use it.

Claude Code passes your plan limits to its status line command while it runs in a terminal.
Connected from the dashboard's Limits section, agent-dashboard becomes that command and records each reading; if you already have a status line, it records the limits and then shows yours.
The dashboard shows the change to Claude Code's `settings.json` before making it, keeps a backup, and puts your own status line back when you disconnect.
The status line runs a copy of agent-dashboard kept with the history, so updating the extension never breaks it.

Anthropic's undocumented usage endpoint can fill the gaps when no Claude Code session runs in a terminal.
It is off unless you turn it on in Settings, labelled experimental, and reads the login Claude Code keeps on your computer without changing it.
Anthropic's terms say Claude Code's login is for Claude Code and Anthropic's own apps, so decide for yourself whether to turn it on.

## Cost

Claude Code's cost is an API-equivalent estimate at Anthropic's list prices, not what your plan charges.
Copilot's cost, in Copilot CLI and in VS Code, is the charge Copilot records for each request, at $0.01 an AI credit.
A figure that adds Claude Code's cost to Copilot's is labelled as including an estimate.

## Settings

| Setting | What it does |
|---|---|
| `agentDashboard.server.path` | The `agent-dashboard` program to run instead of the one that comes with the extension. Only your user or remote settings can set it. |

The dashboard's own Settings section holds the rest: data folders, where plan limits come from, the history, and your time zone and first day of the week.

## Privacy

- agent-dashboard reads the files Claude Code, Copilot CLI and VS Code keep on your machine and records numbers, names and titles in a local database; it never stores prompt or code content.
- There is no telemetry.
- It only uses the network for features you switch on: plan limits from Anthropic's undocumented usage endpoint (labelled experimental), and the daily check for a newer price table in this repository.

## Platforms

Windows, macOS and Linux (including Alpine), on x64 and arm64, with VS Code 1.126 or later, Cursor, Windsurf and VSCodium.
On any other platform, set `agentDashboard.server.path` to a `agent-dashboard` you build or install yourself.

## Licence

[MIT](https://github.com/philll11/agent-dashboard/blob/main/LICENSE).
Dashboard for Coding Agents is an independent project and is not affiliated with or endorsed by Anthropic.
