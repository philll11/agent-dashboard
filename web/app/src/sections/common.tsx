// What every section shares: its heading and filters, the days it covers, and how totals read.

import { type ReactNode, useMemo } from "react";

import type { CostProvenance, Days, Filters, Overview, Totals } from "@agent-dashboard/protocol";

import { AGENTS, type AgentOverview, recordsCharges } from "../agents";

import { useHost, useStatus } from "../context";
import { useRequest } from "../data";
import { fullDate, grouped, money } from "../format";
import { type Series, seriesOf } from "../models";
import { type Choice, Picker, Segmented } from "../ui/controls";
import { type Range, RANGE_NAMES, RANGES, rangeDays, useView } from "../view";

/** The days the chosen range covers. */
export function useDays(): Days {
  const status = useStatus();
  const { view } = useView();
  return rangeDays(view.range, status);
}

/** The range in a subtitle: `last 30 days`. */
export function useRangeText(): string {
  const { view } = useView();
  return RANGE_NAMES[view.range].long.toLowerCase();
}

/** What an export of `count` rows of `noun`s holds, for its menu: `All 6 projects in the last 30
 * days`, `All 214 sessions that match`. */
export function useExportScope(count: number, one: string, many: string, after?: string): string {
  const { view } = useView();
  const what = `${grouped(count)} ${count === 1 ? one : many}`;
  if (after) return `All ${what} ${after}`;
  if (view.range === "all-time") return `All ${what}`;
  return `All ${what} ${RANGE_NAMES[view.range].phrase}`;
}

/** What an export of the days holds, for its menu: `Every day with requests in the last 30
 * days`. */
export function useDaysScope(): string {
  const { view } = useView();
  const status = useStatus();
  if (view.range === "today") return "Today's requests, in one row";
  if (view.range === "all-time") {
    return status.firstDay
      ? `Every day with requests since ${fullDate(status.firstDay)}`
      : "Every day with requests";
  }
  return `Every day with requests ${RANGE_NAMES[view.range].phrase}`;
}

/** A section's heading, with a line about what it shows and its filters. In VS Code the tab
 * names the section, so the heading is for screen readers only. */
export function SectionHead({
  title,
  summary,
  children,
}: {
  title: string;
  summary?: ReactNode;
  children?: ReactNode;
}) {
  const host = useHost();
  if (host.surface === "vscode" && !summary && !children) {
    return <h1 className="visually-hidden">{title}</h1>;
  }
  return (
    <div className="section-head">
      <div className="section-titles">
        <h1 className={host.surface === "desktop" ? undefined : "visually-hidden"}>{title}</h1>
        {summary && <span className="subtitle">{summary}</span>}
      </div>
      <div className="filters">{children}</div>
    </div>
  );
}

const RANGE_CHOICES: Choice<Range>[] = RANGES.map((range) => ({
  value: range,
  label: RANGE_NAMES[range].short,
}));

const RANGE_MENU: Choice<Range>[] = RANGES.map((range) => ({
  value: range,
  label: RANGE_NAMES[range].long,
}));

/** The date range as a row of buttons, or as a menu where the section has no room for the row. */
export function RangeButtons() {
  const { view, change } = useView();
  return (
    <>
      <div className="range-buttons">
        <Segmented
          label="Date range"
          value={view.range}
          choices={RANGE_CHOICES}
          onChange={(range) => {
            change({ range });
          }}
        />
      </div>
      <div className="range-menu">
        <RangeMenu />
      </div>
    </>
  );
}

/** The date range as a menu. */
export function RangeMenu() {
  const { view, change } = useView();
  return (
    <Picker
      label="Date range"
      value={view.range}
      choices={RANGE_MENU}
      onChange={(range) => {
        change({ range });
      }}
    />
  );
}

/** The agent picker, once the history holds more than one agent, or while one is chosen. */
export function AgentMenu() {
  const { view, change } = useView();
  const choices = useRequest("choices", null).data;
  const known = choices?.agents ?? [];
  if (known.length < 2 && view.agent === null) return null;
  const agents: Choice<string | null>[] = [
    { value: null, label: "All agents" },
    ...known.map((agent) => ({ value: agent.id, label: agent.name })),
  ];
  if (view.agent !== null && !agents.some((choice) => choice.value === view.agent)) {
    agents.push({ value: view.agent, label: view.agent });
  }
  return (
    <Picker
      label="Agent"
      value={view.agent}
      choices={agents}
      onChange={(agent) => {
        change({ agent });
      }}
    />
  );
}

export function ProjectMenu() {
  const { view, change } = useView();
  const choices = useRequest("choices", null).data;
  const projects: Choice<string | null>[] = [
    { value: null, label: "All projects" },
    ...(choices?.projects ?? []).map((project) => ({ value: project.id, label: project.name })),
  ];
  if (view.project !== null && !projects.some((choice) => choice.value === view.project)) {
    projects.push({ value: view.project, label: view.project });
  }
  return (
    <Picker
      label="Project"
      value={view.project}
      choices={projects}
      onChange={(project) => {
        change({ project });
      }}
      mono
    />
  );
}

export function ModelMenu() {
  const { view, change } = useView();
  const choices = useRequest("choices", null).data;
  const models: Choice<string | null>[] = [
    { value: null, label: "All models" },
    ...(choices?.models ?? []).map((model) => ({ value: model.id, label: model.name })),
  ];
  if (view.model !== null && !models.some((choice) => choice.value === view.model)) {
    models.push({ value: view.model, label: view.model });
  }
  return (
    <Picker
      label="Model"
      value={view.model}
      choices={models}
      onChange={(model) => {
        change({ model });
      }}
    />
  );
}

/** The filters the view holds, as the server takes them. */
export function useFilters(): {
  agent: string | null;
  project: string | null;
  model: string | null;
} {
  const { view } = useView();
  return { agent: view.agent, project: view.project, model: view.model };
}

/** The agents the view covers: the one chosen, or every agent in the history. */
export function useAgents(): string[] {
  const { view } = useView();
  const choices = useRequest("choices", null).data;
  if (view.agent !== null) return [view.agent];
  return choices?.agents.map((agent) => agent.id) ?? [];
}

/** Each agent's figures for `days` under `filters`, in the agents' order, while the view covers
 * more than one agent: usage is then split by agent before anything else, since each agent may be
 * paid for by someone else. `undefined` while the view covers one agent; empty until every answer
 * is in. */
export function useByAgent(days: Days, filters: Filters): AgentOverview[] | undefined {
  const agents = useAgents();
  const split = agents.length > 1;
  const asked = (agent: string) => split && agents.includes(agent);
  // One request per agent this version knows, always in the same order, each asked while its
  // agent is in view.
  const answers: Record<string, Overview | undefined> = {
    "claude-code": useRequest(
      "overview",
      { days, filters: { ...filters, agent: "claude-code" } },
      asked("claude-code"),
    ).data,
    "copilot-cli": useRequest(
      "overview",
      { days, filters: { ...filters, agent: "copilot-cli" } },
      asked("copilot-cli"),
    ).data,
    "copilot-vscode": useRequest(
      "overview",
      { days, filters: { ...filters, agent: "copilot-vscode" } },
      asked("copilot-vscode"),
    ).data,
  };
  if (!split) return undefined;
  const shown = AGENTS.filter((agent) => agents.includes(agent));
  const groups: AgentOverview[] = shown.flatMap((agent) => {
    const overview = answers[agent];
    return overview ? [{ agent, overview }] : [];
  });
  return groups.length === shown.length ? groups : [];
}

/** Where the costs of `totals` come from; without any cost, where the agents in view would take
 * theirs from: Claude Code from estimates, the Copilot agents from their recorded charges. */
export function useProvenance(totals: Totals | undefined): CostProvenance {
  const agents = useAgents();
  if (totals?.costProvenance) return totals.costProvenance;
  const charges = agents.some((agent) => recordsCharges(agent));
  const estimates = agents.some((agent) => !recordsCharges(agent));
  if (!charges) return "estimate";
  return estimates ? "mixed" : "recorded";
}

/** A cost's badge, by where it comes from. */
export const PROVENANCE_BADGE: Record<CostProvenance, string> = {
  estimate: "Estimate",
  recorded: "Recorded",
  mixed: "Includes estimate",
};

/** What a cost is, after its figure in a summary: `$148.37 API-equivalent`. */
export const COST_WORDS: Record<CostProvenance, string> = {
  estimate: "API-equivalent",
  recorded: "recorded",
  mixed: "including estimates",
};

/** What a cost is called, by where it comes from: `API-equivalent cost` while it is all
 * estimate. */
export function costName(provenance: CostProvenance): string {
  return provenance === "estimate" ? "API-equivalent cost" : "Cost";
}

/** Where a cost comes from, for a subtitle. */
export function costSource(provenance: CostProvenance): string {
  switch (provenance) {
    case "estimate":
      return "Estimate at Anthropic's list prices";
    case "recorded":
      return "As Copilot recorded it";
    case "mixed":
      return "Includes an estimate at Anthropic's list prices";
  }
}

/** A count the agent does not record, such as a VS Code prompt's input tokens, which never shows
 * as 0. */
export function NotRecorded() {
  return (
    <>
      <span aria-hidden="true" className="muted">
        –
      </span>
      <span className="visually-hidden">not recorded</span>
    </>
  );
}

/** Whether every request of `totals` is a prompt from VS Code's chat session files, whose input
 * and cache tokens are not recorded. */
export function onlyPrompts(totals: Totals): boolean {
  return totals.requests > 0 && totals.perPromptRequests === totals.requests;
}

/** What the token figures leave out where some requests are VS Code's prompts. */
export function promptsNote(prompts: number): string {
  return `VS Code's chat session files record a prompt's output tokens and charge alone, so the input and cache tokens leave out ${grouped(prompts)} ${prompts === 1 ? "prompt" : "prompts"}.`;
}

/** Every token, of every kind. */
export function tokensOf(totals: Totals): number {
  const { input, output, cacheRead, cacheWrite5m, cacheWrite1h } = totals.tokens;
  return input + output + cacheRead + cacheWrite5m + cacheWrite1h;
}

/** The share of input tokens read from the prompt cache. */
export function cacheShare(totals: Totals): number {
  const { input, cacheRead, cacheWrite5m, cacheWrite1h } = totals.tokens;
  const all = input + cacheRead + cacheWrite5m + cacheWrite1h;
  return all > 0 ? cacheRead / all : 0;
}

/** A cost, saying when some requests could not be priced: `$5.05 + unpriced`. */
export function costText(totals: Totals): string {
  if (totals.requests > 0 && totals.unpricedRequests === totals.requests) return "Unpriced";
  return totals.unpricedRequests > 0
    ? `${money(totals.cost.total)} + unpriced`
    : money(totals.cost.total);
}

/** A change as a signed whole percentage, with a real minus sign: `+12%`, `−8%`. */
export function signedPercent(now: number, before: number): string | undefined {
  if (before <= 0) return undefined;
  const change = Math.round(((now - before) / before) * 100);
  return `${change >= 0 ? "+" : "−"}${String(Math.abs(change))}%`;
}

/** The colour series of every model, the same whatever the filters: each family's newest model in
 * the history gets the family's colour. */
export function useSeries(): Map<string, Series> {
  const choices = useRequest("choices", null).data;
  return useMemo(() => seriesOf(choices?.models.map((model) => model.id) ?? []), [choices]);
}
