# Copilot in VS Code is an agent of its own

VS Code keeps a person's Copilot sessions in one of two places, by the harness that runs them.
A session on the Copilot harness, VS Code's default since [version 1.143](https://code.visualstudio.com/updates/v1_143), is kept where Copilot CLI keeps its own, under `session-state` in `~/.copilot`, with the same usage records, and its `workspace.yaml` names VS Code as the client that started it: `vscode-agent-host`, or `vscode` for VS Code's earlier integration of Copilot CLI.
A session on the Local harness, which ran every session before 1.143, is kept in VS Code's own chat session files; [VS Code 1.146](https://code.visualstudio.com/updates/v1_146) deprecates that harness and removes it in February 2027, so these files are the only record of that history.
The dashboard reads both as one agent, Copilot in VS Code, because an agent answers where the person used Copilot; the agent filter keeps it apart from Copilot CLI.

A session in `~/.copilot` belongs to Copilot in VS Code when VS Code started it, and to Copilot CLI otherwise.
The history stores such a session with the agent version 2.0 knows, `copilot-cli`, and the harness VS Code ran it on, so a 2.0 surface still reads the history and shows those sessions as Copilot CLI's, with their charges; like any older version on an upgraded history, it records nothing new until it is updated.
A session only ever moves from Copilot CLI to Copilot in VS Code, never back, so an unreadable `workspace.yaml` cannot move it; the upgrade moves the sessions 2.0 recorded by the client their requests came from, so sessions whose files are gone move too.
Readers leave out an agent they do not know instead of reading it as Claude Code, so a later agent can be stored as itself.

A chat session file is a log of changes to the session that only makes sense from its first line, and VS Code rewrites it from a fresh copy every 512 entries ([`objectMutationLog.ts`](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/chat/common/model/objectMutationLog.ts)), so a changed file is read again from its start and replayed as VS Code applies it.
It holds one request per prompt, covering every model call the prompt led to, with the prompt's output tokens and its charge in AI credits, which VS Code records since [version 1.126](https://code.visualstudio.com/updates/v1_126), and without its input or cache tokens ([`chatModel.ts`](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/chat/common/model/chatModel.ts)).
Each finished prompt is therefore one request in the history, marked as per prompt: every surface shows its input and cache tokens as not recorded, never as 0, and says how many prompts a total leaves out.
A prompt that leaves the file, edited or undone, stays in the history, because it was charged; a copy of the session in another workspace's folder meets the original on the request id.

Both Copilot agents' charges are recorded, as ADR 0015 decides for Copilot CLI: a harness session's by kind of token, and a Local prompt's as one total; a prompt without a charge is unpriced.
The monthly allowance is the Copilot plan's, so it shows whenever either Copilot agent is in view, recorded from Copilot's session files; VS Code's own files hold none.
Model ids match Copilot CLI's, so a model has one name under both Copilot agents.
A session's project is the folder its workspace names, shared with the other agents by path; on Windows a project is found by path regardless of case, as Windows treats folders.

VS Code's user data folder is found as VS Code finds it, from `VSCODE_PORTABLE`, then `VSCODE_APPDATA`, then the platform's default, with Insiders' beside it when it exists.
`AGENT_DASHBOARD_VSCODE_DATA` names one folder instead, for tests among others: VS Code honours `VSCODE_APPDATA` ahead of `--user-data-dir`, so setting it would also move the settings of a VS Code that a test runs.

## Considered options

- **One Copilot agent** for the CLI and VS Code would merge the two places a person uses Copilot, so the agent filter could not say what each costs.
- **An agent per harness** would split one person's VS Code sessions by a runtime detail that VS Code chooses, and move a session between agents when VS Code moves it between harnesses.
- **Storing `copilot-vscode` as each session's agent** would read naturally, but a 2.0 surface on the same history would take those sessions for Claude Code's and price them from Anthropic's table.
- **VS Code's OpenTelemetry export and debug logs** are off by default and hold only what happens while they are on.
- **GitHub's billing reports** go no finer than a day and a model, with nothing per editor or session for individual plans ([billing for individuals](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)).
- **Parsing the charge from the text under a response**, the only record of a prompt's AI credits between their start on 1 June 2026 and VS Code 1.126, would depend on a string that is localised and rounded.
- **Reading only what a chat session file appended** would miss VS Code's rewrite and the entries that cut or delete earlier ones.

## Consequences

- Records of one request that tie on tokens are ordered by their charge, so a prompt's newest reading wins while its charge grows (ADR 0003).
- A session moved from the Local harness to the Copilot harness keeps its earlier prompts' usage in the Local file only, since VS Code rebuilds the moved turns without usage records ([microsoft/vscode#345984](https://github.com/microsoft/vscode/pull/345984)), so nothing is counted twice.
- Inline chat and completions leave no usage on disk, so they are not counted.
- In a remote window the Copilot harness keeps its sessions on the remote machine: reachable for WSL as an added folder, not for SSH, containers or Codespaces.
