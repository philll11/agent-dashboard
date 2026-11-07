# Agent Dashboard

Agent Dashboard shows a person how they use their coding agents, Claude Code and GitHub Copilot CLI: what their usage costs, where it goes, and how close they are to their plan limits.
It keeps that history after the agents delete their own transcripts.

## Language

### The agents' data

**Agent**:
A coding assistant whose usage we read: Claude Code or Copilot CLI.
_Avoid_: tool, source, provider, client

**Data root**:
A folder in which an agent keeps its transcripts, such as Claude Code's `~/.claude` or Copilot CLI's `~/.copilot`, unless the person has moved it.
_Avoid_: config dir, Claude folder

**Transcript**:
The file in which an agent records one session, or for Claude Code, one subagent's part of a session.
A Copilot CLI session's `events.jsonl` is its transcript.
_Avoid_: log, JSONL file, conversation file

**Session**:
One conversation with an agent, identified by its session id.
_Avoid_: conversation, chat, thread

**Running session**:
A session still under way: it has made requests lately, and for Claude Code, Claude Code has not yet recorded the cost it records as the session's process ends.
_Avoid_: active session, live session, open session

**Subagent**:
A helper agent that a session starts; its requests belong to the session that started it.
Copilot CLI calls it a sub-agent.
_Avoid_: sidechain, task, agent (on its own)

**Project**:
The working folder a session ran in, shared by every agent that worked there.
_Avoid_: workspace, repo, cwd

**Request**:
One call from an agent to a model, with the tokens it used.
_Avoid_: turn, message, entry, API call

**Initiator**:
Who started a request, where the agent records it: the person's own prompt, the agent on its own, or a subagent.
_Avoid_: caller, trigger

**Attribution**:
The skill, MCP server or MCP tool that Claude Code credits a request to.
_Avoid_: source, origin

### Our history

**History**:
Every request and session we have recorded, kept after the agents delete the transcripts they came from.
_Avoid_: archive, warehouse, cache

**Ingest**:
Reading new transcript content into the history.
_Avoid_: scan, sync, import

**Import**:
A one-time transfer into the history from another source, such as the claude-usage dashboard's database.
_Avoid_: ingest, migration

**Daily totals**:
Tokens per model per day without request detail, used only for days that no transcript covers.
_Avoid_: summary, rollup

### Cost

**Cost**:
A request's money figure in US dollars: its charge where the agent records one, and otherwise its API-equivalent cost.
Costs add up across agents into one figure that says whether it includes an estimate.
_Avoid_: spend, bill

**API-equivalent cost**:
What Claude Code's requests would cost at Anthropic's published API prices; an estimate, never a bill.
_Avoid_: cost (on its own), spend, bill, price

**Charge**:
What a request cost, as the agent recorded it from its provider's billing, such as Copilot CLI's AI credits.
_Avoid_: bill, spend, price

**AI credit**:
GitHub's unit of Copilot usage, worth $0.01.
_Avoid_: premium request, AIU (outside code)

**Price table**:
The dated list of API prices per model from which API-equivalent cost is calculated: the one a version ships with, or a newer one from the project's repository.
_Avoid_: pricing, rates, rate card

**Unpriced**:
A request with neither a price in the price table nor a charge, such as a request to a model the table does not list, or Copilot CLI's on the person's own key; its cost is unknown, never zero.
_Avoid_: unknown model, free model

**Cache write**:
Tokens that an agent stores in a provider's prompt cache, priced by how long they are kept: five minutes or one hour.
_Avoid_: cache creation

**Cache read**:
Tokens served from a provider's prompt cache instead of being processed again.
_Avoid_: cache hit

**Reasoning tokens**:
Output tokens a model spends thinking before it answers, which Copilot CLI records apart; they are part of the output, never added to it.
_Avoid_: thinking tokens

**Reported session cost**:
The cost Claude Code records for a session itself, also calculated at list price.
_Avoid_: actual cost, real cost, true cost

**Reconciliation**:
Comparing a Claude Code session's API-equivalent cost with its reported session cost.
_Avoid_: validation, verification

### Plan limits

**Plan**:
The subscription whose usage limits apply to a person: a Claude plan, such as Pro or Max, or a Copilot plan.
_Avoid_: tier, account

**Plan window**:
A period over which a plan limit is measured: Claude Code's five-hour window and weekly windows, or Copilot's monthly allowance.
_Avoid_: block, session limit, rate limit

**Monthly allowance**:
The AI credits a Copilot plan includes each calendar month, reset at 00:00 UTC on the 1st, counting Copilot wherever the person uses it.
_Avoid_: quota, premium requests

**Limit snapshot**:
How much of a plan window was used and when it resets, as reported at one moment.
_Avoid_: quota, usage bar

**Provenance**:
Where a number comes from.
For plan limits: official (documented by the agent's maker, Claude Code's status line), recorded (from the agent's own files, Copilot CLI's allowance) or experimental (Anthropic's undocumented usage endpoint).
For cost: estimate (our price table), recorded (the agent's charges) or mixed (both).
_Avoid_: source, confidence

### Product

**Surface**:
One of the three ways the product is shown: the terminal, the desktop app or VS Code.
_Avoid_: client, front end, app (on its own)

**Glance**:
Plan limits and today's cost in a few words: the desktop app's item in the notification area or the menu bar, and VS Code's status bar item.
_Avoid_: tray (on its own), widget, indicator

**Export**:
A section's table, or every request in the history, written as CSV or JSON for a spreadsheet or a script.
_Avoid_: download, dump, report (for a file)
