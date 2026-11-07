// The coding agents whose usage the dashboard shows, by the ids the server gives them.

const NAMES: Record<string, string> = {
  "claude-code": "Claude Code",
  "copilot-cli": "Copilot CLI",
};

/** An agent's name as people read it, such as `Copilot CLI`; the id for an agent this version
 * does not know. */
export function agentName(id: string): string {
  return NAMES[id] ?? id;
}
