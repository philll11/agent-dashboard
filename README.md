# agent-dashboard

See how you use your coding agents, Claude Code, GitHub Copilot CLI and GitHub Copilot in VS Code: what your usage costs, where it goes, and how close you are to your plan limits.
It runs inside VS Code, as a desktop app and in the terminal, on Windows, macOS and Linux, with nothing else to install.

> It is made for personal use, and installed from the [releases](https://github.com/philll11/agent-dashboard/releases) rather than a marketplace.
> What each version keeps stable is under [Versions](#versions).

## What it is for

- **Plan limits at a glance**: how much of Claude Code's five-hour and weekly windows and Copilot's monthly allowance you have used, and when they reset.
- **Usage and cost**: tokens and cost today, this week and this month, in one figure that says whether it includes an estimate, and each agent's models and projects apart, since the agents are often paid for by different people.
- **Where it goes**: projects, sessions, subagents, skills and MCP servers, and how well the prompt cache works for you.
- **History that lasts**: Claude Code deletes its transcripts after 30 days by default; agent-dashboard keeps what it has read.
- **Your data, your way**: CSV and JSON export, and terminal reports you can script.

## Where it runs

| Surface | What you get |
|---|---|
| VS Code | A status bar item, a sidebar summary, and the full dashboard in the panel beside the terminal and in an editor tab. Works over WSL and SSH. |
| Desktop | The full dashboard in its own window on Windows, macOS and Linux. |
| Terminal | An interactive terminal UI, plus reports with `--json` and `--csv` output. |

## In VS Code

Install **Dashboard for Coding Agents** from the [latest release](https://github.com/philll11/agent-dashboard/releases/latest): download the package for your platform, such as `agent-dashboard-win32-x64-<version>.vsix` on Windows, and install it with **Extensions: Install from VSIX…** or `code --install-extension <package>`.
The platforms are `win32`, `darwin`, `linux` and `alpine`, each `x64` or `arm64`; Cursor, Windsurf and VSCodium install the same packages.
It comes with the `agent-dashboard` program for your platform, so nothing else needs installing.
To update, install the package from a newer release.

- The status bar shows Claude Code's five-hour window and when it resets, and Copilot's allowance, such as `5h 42% · 14:20 · Copilot 4%`, or today's cost before plan limits are recorded.
- Its sidebar lists plan limits, today by model, this week and the week's top projects.
- The **Usage dashboard** tab in the panel, beside the terminal, holds the whole dashboard under a strip that keeps Claude Code's five-hour and weekly windows, Copilot's allowance and the latest session in the project you have open in sight, whichever section you look at.
  Drag it beside the terminal or into a sidebar, and it fits itself to the room.
- The dashboard also opens in an editor tab, in your colour theme: Overview, Sessions, Projects, Models, Limits and Settings.
- Its walkthrough records your plan limits, brings in claude-usage history and chooses data folders.

In a window connected to WSL, over SSH or in a container, it runs on that machine and reads its agents' folders.
Install it there from that window, with **Extensions: Install from VSIX…** and the package for that machine, such as `agent-dashboard-linux-x64-<version>.vsix` for WSL on an x64 computer: an extension installed in a local window does not run in remote ones until it is installed there too.
`agentDashboard.server.path` runs another `agent-dashboard` instead of the one that comes with the extension, such as a build of your own.

## On the desktop

Install **Dashboard for Coding Agents** from the [latest release](https://github.com/philll11/agent-dashboard/releases/latest) with the installer for your system, such as `agent-dashboard-desktop-<version>-windows-x64-setup.exe`:

- Windows: the `-setup.exe`, which installs for you alone and needs no administrator rights.
- macOS: the `.dmg`.
- Linux: the `.AppImage`, or the `.deb` package.

Each comes for x64 and arm64.
It installs updates from later releases by itself, signed and checked before installing; Settings can leave that to you instead.

The installers are not signed.
Windows SmartScreen asks once before the downloaded installer runs: choose **More info**, then **Run anyway**.
macOS asks once before the app opens: in **System Settings**, **Privacy & Security**, choose **Open Anyway**.

- The sidebar holds the sections: Overview, Sessions, Projects, Models, Limits and Settings.
- The window is light or dark as your system is, and remembers its size and place.
- Its item in the notification area, or the menu bar on macOS, shows how much of the five-hour window you have used, marked once that or Copilot's allowance passes 70%, and its menu lists every plan window and today's cost.
  On Windows and macOS, closing the window keeps the app running there, and **Quit** in the item's menu ends it; on Linux, closing the window quits unless you switch keeping it running on in Settings.
  Settings can also hide the item and open the app at login, in the item without the window.
- Each section's **Export** button saves its table as CSV or JSON, and Settings saves the whole history.
- `agent-dashboard app` opens it from a terminal, and `agent-dashboard app limits` at a section.

It comes with the `agent-dashboard` program, which Claude Code's status line runs a copy of once you connect it, so updating or removing the app never breaks the status line.
If something goes wrong, the app's log is in `%LOCALAPPDATA%\io.github.philll11.agent-dashboard\logs` on Windows, `~/Library/Logs/io.github.philll11.agent-dashboard` on macOS and `~/.local/share/io.github.philll11.agent-dashboard/logs` on Linux.

## In the terminal

Install it with the installer for your system, and run the installer again to update.
On macOS and Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/philll11/agent-dashboard/releases/latest/download/agent-dashboard-installer.sh | sh
```

On Windows:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/philll11/agent-dashboard/releases/latest/download/agent-dashboard-installer.ps1 | iex"
```

Both installers also put `agd` beside it: a short name for the same program, which takes every command and option, so `agd` opens the dashboard and `agd today` prints today's report.
The binaries are not signed.
Installed this way they carry no browser download mark, so Windows SmartScreen and macOS Gatekeeper do not warn about them; Windows Smart App Control, where it is on, blocks unsigned programs however they arrive.
[Releases](https://github.com/philll11/agent-dashboard/releases) lists every version with its notes and checksums.
To build from source instead, with Rust 1.99 or later, which installs it as `agent-dashboard` alone:

```sh
cargo install --locked --git https://github.com/philll11/agent-dashboard agent-dashboard
```

### The dashboard in the terminal

`agent-dashboard` on its own, or `agd`, opens the dashboard in the terminal: Overview, Sessions with each session's detail, Projects, Models and Limits.
It works from the keyboard alone, in a terminal of 80 by 24 or larger:

| Keys | What they do |
|---|---|
| `←` `→`, `Tab`, `1` to `5` | Change the section |
| `↑` `↓` or `j` `k`, `PgUp` `PgDn`, `Home` `End` | Move in a list, or scroll a page |
| `Enter` | Open a session, or filter the dashboard to a project |
| `Esc` | Go back, or close |
| `/` | Search the sessions |
| `r`, `a`, `p`, `m` | Choose the range, the agent, the project and the model |
| `t` | Show the charts as tables |
| `e` | Export the section shown, as CSV or JSON, to a file in your Downloads folder unless you change it |
| `R` | Read new transcripts now |
| `?` | Show the keys |
| `q`, `Ctrl-C` | Quit |

It reads what the agents add to their transcripts every minute and updates in place.
Its colours suit your terminal's light or dark background, which it asks the terminal about; `AGENT_DASHBOARD_THEME=light` or `dark` chooses instead.
`NO_COLOR` turns colour off, and the charts then tell the models apart by pattern.
In Git Bash's own window, run it in Windows Terminal or through `winpty agent-dashboard`.
In the Windows console host and the Linux console, whose fonts lack the finer block characters, sparklines step in half cells.
For a screen reader, the reports below print plain lines.

### Reports

| Command | What it shows |
|---|---|
| `agent-dashboard today` | Today's requests and cost by model, and your plan limits |
| `agent-dashboard daily` | Requests, tokens and cost per day |
| `agent-dashboard weekly` | Requests, tokens and cost per week |
| `agent-dashboard monthly` | Requests, tokens and cost per calendar month |
| `agent-dashboard models` | Requests and cost by model |
| `agent-dashboard sessions` | Requests and cost by session |
| `agent-dashboard projects` | Requests and cost by project |
| `agent-dashboard limits` | Each plan window's use, reset and source, and the recent five-hour windows |
| `agent-dashboard export` | Every request, or those of the days given, as CSV or JSON |
| `agent-dashboard prices` | Which price table is in use, and the daily check for a newer one |
| `agent-dashboard doctor` | What agent-dashboard can find and read, and anything that needs your attention |
| `agent-dashboard app` | Opens the desktop app, at a section when you name one |

`daily`, `models` and `projects` cover the last 30 days and `sessions` the last 7, unless you give `--since` and `--until` with dates such as `2026-10-05`.
`weekly` and `monthly` cover the last 12 weeks and months, and with `--since` and `--until` every week or month those days fall in, whole; weeks start on the day the dashboard's settings name, Monday unless you choose another.
Every report takes `--json` or `--csv` for scripts, and `--agent claude-code`, `--agent copilot-cli` or `--agent copilot-vscode` for one agent's usage.
Each command first reads what the agents have added to their transcripts; the first time, that takes a few seconds.

### Export

`agent-dashboard export` writes one row per request, with its agent, session, project, model, tokens, tools and cost, and where the cost comes from: as CSV unless you give `--json`, for every day unless you give `--since` and `--until`.
A request from VS Code's own chat session files is a whole prompt, marked `per_prompt`, with its input and cache tokens left empty, since VS Code does not record them.
It writes to standard output for scripts, or with `--output requests.csv` to a file made for spreadsheets: a CSV file starts with UTF-8's byte order mark, so that Excel reads it as UTF-8, and text that a spreadsheet would take for a formula, such as a session title starting with `=`, gets an apostrophe before it.
Use `--output` in Windows PowerShell 5.1, whose `>` writes another encoding.
The dashboard's exports are the same files.
Days are written in your time zone, moments in UTC, and costs in US dollars without a symbol.

### Cost

Claude Code's cost is an API-equivalent estimate: what its requests would cost at Anthropic's published API prices, never what your plan charges.
Copilot's cost, in Copilot CLI and in VS Code, is what Copilot recorded for each request in AI credits, at $0.01 a credit, including what GitHub adds, such as for data residency.
A figure that adds Claude Code's cost to Copilot's says it includes an estimate, and the agent filter splits it.

### Prices

API-equivalent cost comes from the price table in this repository's `pricing/prices.json`, which each release brings.
`agent-dashboard prices refresh on`, or the switch in the dashboard's Settings, has a running dashboard check the repository for a newer table once a day, and every surface prices with it; `agent-dashboard prices refresh off` stops the checks.
`agent-dashboard prices update` checks once, at once, and `agent-dashboard prices` says which table is in use and how the last check went.
A newer table is used only when it keeps every price this version has, adding only new models and prices.

### Plan limits

Copilot notes your monthly allowance, in AI credits, in its session files with each request it makes, so there is nothing to connect.
The allowance counts Copilot wherever you use it, and resets at 00:00 UTC on the 1st of each month.

Claude Code passes your plan limits to its status line command while it runs in a terminal.
`agent-dashboard statusline connect` makes agent-dashboard that command, after showing you the change to Claude Code's `settings.json` and keeping a backup.
Its line starts with the session's title, which tells apart sessions running side by side, then shows the model, how much of each plan window is used and Claude Code's API-equivalent cost today: `Fix the importer · Opus 5.5 · 5h 42% · week 71% · $5.05 today`.
If you already have a status line, agent-dashboard records the plan limits and then shows yours.
`agent-dashboard statusline disconnect` puts back the status line you had before.

When no Claude Code session runs in a terminal, Anthropic's undocumented usage endpoint can fill the gaps.
`agent-dashboard limits endpoint on` turns it on once you agree, and `agent-dashboard limits endpoint off` turns it off.
It reads the login Claude Code keeps on your computer, never changes or refreshes it, and sends it only to `api.anthropic.com`, at most once every 5 minutes.
Requests go through the proxy that `ALL_PROXY`, `HTTPS_PROXY` or `HTTP_PROXY` names unless `NO_PROXY` excludes the host, or on Windows the system proxy, and trust the operating system's certificates.
Anthropic's terms say Claude Code's login is for Claude Code and Anthropic's own apps, and that Anthropic may act on other uses without notice, so decide for yourself whether to turn it on.

### History from claude-usage

If you used the [claude-usage](https://github.com/phuryn/claude-usage) dashboard, `agent-dashboard import claude-usage` brings in the requests from its database that the history does not have, such as those from transcripts Claude Code has since deleted.

### Copilot in VS Code

VS Code runs each Copilot session in its Chat view on one of two harnesses, and agent-dashboard reads both as Copilot in VS Code.
A session on the Copilot harness, VS Code's default, is kept with Copilot CLI's in `~/.copilot`, with the same tokens and charges.
A session on the Local harness, which VS Code removes in February 2027, is kept in VS Code's own chat session files, which record each prompt's output tokens and charge but not its input or cache tokens: those show as not recorded, and totals say how many prompts they leave out.
VS Code records a prompt's charge from version 1.126, so earlier prompts are unpriced.
Inline chat and code completions leave no usage on disk, so they are not counted.

In a window connected to WSL, over SSH or in a container, the Copilot harness keeps its sessions on that machine, where the extension installed in that window reads them.
The desktop app and the terminal on your own computer reach WSL's sessions once you add its `~/.copilot` folder in Settings.

### Where the data lives

agent-dashboard reads what Claude Code keeps in `~/.claude`, or in the folders that `CLAUDE_CONFIG_DIR` names, separated by commas, and what Copilot CLI keeps in `~/.copilot/session-state`, or under the folder `COPILOT_HOME` names.
For Copilot in VS Code it reads VS Code's user data folder, and Insiders' too: `%APPDATA%\Code` on Windows, `~/Library/Application Support/Code` on macOS and `~/.config/Code` on Linux, or the folder `VSCODE_PORTABLE` or `VSCODE_APPDATA` points VS Code to; `AGENT_DASHBOARD_VSCODE_DATA` names another.
Settings, or the dashboard's own Settings section, can add another folder for any agent, such as one in WSL, or the folder a VS Code started with `--user-data-dir` uses.
It keeps its own history in `history.sqlite`, in `%LOCALAPPDATA%\agent-dashboard` on Windows, `~/Library/Application Support/agent-dashboard` on macOS and `~/.local/share/agent-dashboard` on Linux.
Set `AGENT_DASHBOARD_HISTORY` to keep it in another file, `NO_COLOR` to turn colours off, and `AGENT_DASHBOARD_THEME` to `light` or `dark` to choose colours for your terminal's background.

## Switching from Dashboard for Claude Code

Before version 2 this dashboard was called claude-dashboard, or Dashboard for Claude Code.
On each machine that has it:

1. Install agent-dashboard as above: the terminal installer, the desktop app and the VS Code package.
   The older desktop app does not update itself to agent-dashboard, so install the new one beside it.
2. Open agent-dashboard.
   The first time, it starts from a copy of claude-dashboard's history, which stays as it is.
3. Switch Claude Code's status line to agent-dashboard: choose **Switch…** in the notice or in Settings, or run `agent-dashboard statusline connect`.
   Until then claude-dashboard records your plan limits in its own history, and switching brings in what it recorded meanwhile.
4. Remove claude-dashboard: the desktop app "Dashboard for Claude Code" (in **Apps & features** on Windows), the VS Code extension of the same name, and `claude-dashboard` and `cld` from `~/.cargo/bin`.
5. Once you are content, delete claude-dashboard's folder: `%LOCALAPPDATA%\claude-dashboard` on Windows, `~/Library/Application Support/claude-dashboard` on macOS or `~/.local/share/claude-dashboard` on Linux.
   Switch the status line first, because claude-dashboard's status line runs the copy of its program kept in that folder.

If you set `CLAUDE_DASHBOARD_HISTORY` or `CLAUDE_DASHBOARD_THEME`, set `AGENT_DASHBOARD_HISTORY` or `AGENT_DASHBOARD_THEME` instead.

## Privacy

- agent-dashboard reads the files Claude Code, Copilot CLI and VS Code keep on your machine and records numbers, names and titles in a local database; it never stores prompt or code content.
- There is no telemetry.
- It only uses the network for features you switch on: plan limits from Anthropic's undocumented usage endpoint (labelled experimental), update checks for the desktop app, and the daily check for a newer price table in this repository.

## How it works

- [`CONTEXT.md`](CONTEXT.md) defines the words used throughout the project.
- [`docs/adr`](docs/adr) records the architecture decisions and why they were made.

## Versions

Every surface has the same version, and the [changelog](CHANGELOG.md) lists what each release changes.
Within a major version, such as 2:

- a later version reads the history an earlier one kept, and an earlier version reads a later one's until the later one changes something it cannot read, when it says to update;
- reports keep their JSON fields and CSV columns, adding new ones at the end;
- commands and their options stay as they are.

## Contributing and security

See [`CONTRIBUTING.md`](CONTRIBUTING.md) to get involved, and [`SECURITY.md`](SECURITY.md) to report a vulnerability privately.

## Licence

[MIT](LICENSE).

agent-dashboard is an independent project and is not affiliated with or endorsed by Anthropic.
