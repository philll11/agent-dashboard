# Product brief

agent-dashboard shows a person how they use their coding agents, Claude Code and GitHub Copilot CLI: how close they are to their plan limits, what their usage costs, and where it goes.
It does so in three surfaces, VS Code, a desktop app and the terminal, from one shared history that outlives the agents' own transcripts.
This brief is the input for the design; the words it uses are defined in [`CONTEXT.md`](../CONTEXT.md).

## Who it is for

- **Pro and Max subscribers** who want to know how close they are to a plan limit, and what is using it up.
- **API users** who want to see what Claude Code costs them, by day, project and model.
- **Copilot subscribers** who want to see how much of their monthly allowance Copilot CLI uses, and on what.
- **People who use both agents** and want one figure for what their agents cost, split by agent when they ask.
- **Heavy users** of subagents, skills and MCP servers who want to know which of them drive their usage.

Claude Code's own `/usage` screen covers the current session, plan usage bars and the last 7 days on one machine.
agent-dashboard adds months of history, comparisons across projects, sessions and time, a cost model that checks itself, and a presence where people already work.

## Jobs, most frequent first

1. **Glance at plan limits.**
   How much of Claude Code's five-hour and weekly windows and Copilot's monthly allowance is used, and when each resets.
2. **Check usage and cost.**
   Tokens and cost today, this week and this month, by agent and model.
3. **See where usage goes.**
   Projects, sessions, subagents, attribution to skills and MCP servers, models, and cache efficiency.
4. **Follow trends.**
   Daily and weekly patterns over months.
5. **Take the data elsewhere.**
   CSV and JSON export, and terminal reports that scripts can use.

## Principles

- **Glance first, detail on demand.**
  The most frequent job is answered without a click; everything else is one step away.
- **Every number says where it comes from.**
  Each figure carries its provenance (official, recorded or experimental for limits; estimate, recorded or mixed for cost) and, for limit snapshots, its age.
- **Honest estimates.**
  API-equivalent cost is always labelled as an estimate, and a figure that adds it to recorded charges says it includes one; an unpriced request shows "unpriced", never $0.
- **At home in each surface.**
  VS Code's theme, including high contrast; the operating system's light or dark mode on the desktop; the terminal's own colours, or none with `NO_COLOR`.
- **Fast from the first run.**
  The last 7 days appear within seconds while older history is still being read.
- **Local and private.**
  No telemetry, no prompt or code content stored, and network use only for features the person switches on.

## Information architecture

### Sections

The desktop app and VS Code's dashboard, in its view and its tab, share these sections; the terminal UI mirrors them.

| Section | Answers | Content |
|---|---|---|
| Overview | Jobs 1 and 2 | Plan window meters by agent with reset times; today, this week and this month (tokens, cost with its provenance, split by agent); daily usage by model for the chosen range; top projects; cache efficiency |
| Sessions | Job 3 | Sessions with title, agent, project, start, duration, models, requests, tokens, cost and, for Claude Code, reported session cost; each opens a session view with its requests over time, models, subagents, initiators for Copilot CLI and attribution for Claude Code |
| Projects | Jobs 3 and 4 | Projects with totals and trend; each opens its sessions and models |
| Models | Jobs 3 and 4 | Tokens and cost per model over time; input, output, reasoning, cache writes and cache reads; fast mode |
| Limits | Job 1 | Current limit snapshots by agent with provenance, age and reset countdown; plan-window use over time; connection status of each limit source |
| Settings | Setup | Data roots by agent, imports, limit connections, price table version, time zone and first day of the week |

Every section filters by date range (today, 7 days, 30 days, this month, all time, custom), agent, project and model, and every table exports to CSV and JSON.

### VS Code

- **Status bar item**: the five-hour window's use and Copilot's allowance when limit snapshots exist, otherwise today's cost; its tooltip gives the details and the age, and clicking it opens the dashboard tab.
- **Sidebar tree**: Plan limits, Today, This week and Top projects; each item opens the matching section of the dashboard tab.
- **Dashboard view**: the full dashboard in the panel beside the terminal, or in a sidebar where the person drags it, under a strip that keeps Claude Code's five-hour and weekly windows, Copilot's allowance and the latest session in the open project in sight whichever section is shown.
- **Dashboard tab**: the full dashboard in an editor tab, restored after VS Code restarts.
- **Walkthrough** for the first run: connect the status line, choose whether to enable the experimental usage endpoint, import history from the claude-usage dashboard, and check data roots.

### Desktop

- One window with the full dashboard, following the operating system's light or dark mode.
- A menu bar or tray item showing the highest current use of the five-hour window and Copilot's allowance.

### Terminal

- `agent-dashboard`, or `agd` where the installers put that short name beside it, opens the terminal UI with the same sections, driven entirely by the keyboard and usable at 80 by 24.
- Reports: `today`, `daily`, `weekly`, `monthly`, `sessions`, `projects`, `models` and `limits`, each with `--json` and `--csv`, and `--agent` to narrow them to one agent.
- `agent-dashboard statusline` prints a compact line for Claude Code's status bar, such as `5h 42% · week 18% · $3.20 today`.
- `agent-dashboard doctor` explains what was found and what is wrong.

## States to design

Every surface handles each of these:

| State | What the person sees |
|---|---|
| First run | History being read, newest first, with progress; data appears as it arrives |
| Empty | No agent's data found, which data roots were checked, and how to add one |
| Import available | A claude-usage database was found, with an offer to import it |
| Limits not connected | What each limit source does, and how to connect it |
| Stale limits | The last limit snapshot with its age, and why it may be old (no terminal session running, for example) |
| Unpriced model | Tokens shown, API-equivalent cost "unpriced", and how to update the price table |
| Prices may be out of date | API-equivalent cost differs from reported session cost beyond rounding |
| Database newer than this version | Read-only mode, with a prompt to update |
| Error | What failed, and the next step |

## Accessibility

- WCAG 2.2 AA in every theme, including VS Code's high-contrast themes.
- Everything works from the keyboard, with visible focus.
- Every chart has a data table for screen readers.
- Motion respects the reduced-motion setting.

## Not in this version

- Team or organisation analytics.
- Usage from claude.ai chat or Cowork, which is not stored on the machine.
- VS Code's Copilot Chat sessions, which Copilot keeps in a different store and format.
- A status line for Copilot CLI, which has none.
- Notifications when a plan window passes a threshold.

## How we know it works

- API-equivalent cost agrees with reported session cost within rounding for every priced model.
- Copilot CLI's recorded charges add up to the totals each of its sessions records.
- On a first run, the last 7 days appear within 3 seconds.
- It installs and runs on Windows, macOS and Linux without anything else installed.
