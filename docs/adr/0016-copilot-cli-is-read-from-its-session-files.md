# Copilot CLI is read from its session files

Copilot CLI keeps each session in a folder of its own under `session-state` in its data root: `workspace.yaml` names the working folder and the title, and `events.jsonl` records the session's events, among them one usage record per model request and the allowance GitHub reports with each model call.
The dashboard reads these files, as it reads Claude Code's transcripts, and they are its only source for Copilot CLI.
The sessions VS Code runs on its Copilot harness are kept here too, and belong to Copilot in VS Code (ADR 0017).
Copilot CLI's monthly allowance is a plan window whose provenance is recorded: it comes from the agent's own files, needs nothing connected, and is as current as Copilot CLI's last request.
This extends ADR 0005, which makes Claude Code's status line the first source of Claude Code's plan limits.

GitHub documents where the files are, not their format, so the reader takes only the events it needs, skips every other line cheaply, and treats a line it cannot read as unreadable rather than as an error.

## Considered options

- **Copilot CLI's OpenTelemetry export** is documented, but holds only what happens while the export is on, never earlier sessions, working folders or the allowance; an enterprise's managed telemetry settings also take the place of a person's own.
  It stays the fallback if the files' format changes beyond reading.
- **GitHub's billing API** reports usage by day, not by session or project, and leaves out licences billed through an organisation.
- **VS Code's Copilot Chat sessions** are a different store in a different format, which ADR 0017 reads as an agent of its own.

## Consequences

- A rewritten events file is read again from its start: the reader checks that the file still begins as it did and that the last position read is a line end.
- The allowance counts Copilot wherever the person uses it, not only the CLI, so the dashboard says so beside it.
- A change to the files' format can stop Copilot CLI's figures from updating; the reader's tests hold synthetic examples of each event it takes.
