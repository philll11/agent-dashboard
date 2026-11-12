// The coding agents whose usage the dashboard shows, by the ids the server gives them, and the
// plans their usage counts against.

import type { Overview } from "@agent-dashboard/protocol";

import { type Series, seriesFill } from "./models";

/** The agents this version reads, in the order every surface lists them. */
export const AGENTS = ["claude-code", "copilot-cli", "copilot-vscode"] as const;

const NAMES: Record<string, string> = {
  "claude-code": "Claude Code",
  "copilot-cli": "Copilot CLI",
  "copilot-vscode": "Copilot in VS Code",
};

/** An agent's name as people read it, such as `Copilot CLI`; the id for an agent this version
 * does not know. */
export function agentName(id: string): string {
  return NAMES[id] ?? id;
}

/** The plans the agents' usage counts against, Claude's first, by the names their windows carry. */
export const PLANS = ["claude", "copilot"] as const;

const PLAN_OF: Record<string, string> = {
  "claude-code": "claude",
  "copilot-cli": "copilot",
  "copilot-vscode": "copilot",
};

/** The plan an agent's usage counts against: Copilot's allowance counts both Copilot agents. */
export function planOf(agent: string): string | undefined {
  return PLAN_OF[agent];
}

/** A plan's heading where windows group by plan: Claude Code's for Claude's, whose windows Claude
 * Code reports, and `Copilot` for Copilot's. */
export function planName(plan: string): string {
  if (plan === "claude") return "Claude Code";
  if (plan === "copilot") return "Copilot";
  return plan;
}

/** Whether an agent records what each request cost, so that its cost is a charge and never an
 * estimate (ADR 0015): both Copilot agents. */
export function recordsCharges(agent: string): boolean {
  return planOf(agent) === "copilot";
}

/** The series an agent takes where a chart shows agents rather than models: the palette's first
 * three slots, in the agents' order, so that no two agents share one. */
const SERIES: Record<string, Series> = {
  "claude-code": "fable",
  "copilot-cli": "opus",
  "copilot-vscode": "sonnet",
};

/** The series an agent's cost is drawn in where a chart shows agents. */
export function agentSeries(id: string): Series {
  return SERIES[id] ?? "other";
}

/** The fill of an agent's swatch, as `seriesFill` gives a model's. */
export function agentFill(id: string): string {
  return seriesFill(agentSeries(id));
}

/** A model's name under its agent's heading: without the `, Copilot` that the server adds to the
 * name of a Copilot model that Claude Code uses too, such as `Opus 5.5, Copilot`. */
export function nameUnder(agent: string, name: string): string {
  const suffix = planOf(agent) === "copilot" ? ", Copilot" : `, ${agentName(agent)}`;
  return name.endsWith(suffix) ? name.slice(0, -suffix.length) : name;
}

/** One agent's figures, where a view splits usage by agent. */
export interface AgentOverview {
  agent: string;
  overview: Overview;
}
