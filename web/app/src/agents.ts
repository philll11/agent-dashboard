// The coding agents whose usage the dashboard shows, by the ids the server gives them.

import type { Overview } from "@agent-dashboard/protocol";

import { type Series, seriesFill } from "./models";

/** The agents this version reads, in the order every surface lists them. */
export const AGENTS = ["claude-code", "copilot-cli"] as const;

const NAMES: Record<string, string> = {
  "claude-code": "Claude Code",
  "copilot-cli": "Copilot CLI",
};

/** An agent's name as people read it, such as `Copilot CLI`; the id for an agent this version
 * does not know. */
export function agentName(id: string): string {
  return NAMES[id] ?? id;
}

/** The series an agent takes where a chart shows agents rather than models: the palette's first
 * two slots, in the agents' order. */
const SERIES: Record<string, Series> = {
  "claude-code": "fable",
  "copilot-cli": "opus",
};

/** The series an agent's cost is drawn in where a chart shows agents. */
export function agentSeries(id: string): Series {
  return SERIES[id] ?? "other";
}

/** The fill of an agent's swatch, as `seriesFill` gives a model's. */
export function agentFill(id: string): string {
  return seriesFill(agentSeries(id));
}

/** A model's name under its agent's heading: without the agent that the server adds to the name
 * of a model both agents use, such as `Opus 5.5, Copilot CLI`. */
export function nameUnder(agent: string, name: string): string {
  const suffix = `, ${agentName(agent)}`;
  return name.endsWith(suffix) ? name.slice(0, -suffix.length) : name;
}

/** One agent's figures, where a view splits usage by agent. */
export interface AgentOverview {
  agent: string;
  overview: Overview;
}
