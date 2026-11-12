// What the status bar item and the sidebar tree show, worked out from the server's answers. This
// module knows nothing of VS Code, so that its tests run without it.

import type {
  Choices,
  Level,
  ModelRow,
  Overview,
  Show,
  Status,
  Totals,
  WindowReading,
} from "@agent-dashboard/protocol";
import {
  type AgentOverview,
  agentName,
  agentSeries,
  nameUnder,
  PLANS,
  recordsCharges,
} from "@agent-dashboard/app/agents";
import { ago, money, percent, when } from "@agent-dashboard/app/format";
import { type Series, seriesOf } from "@agent-dashboard/app/models";
import { creditsText, inOrder, LEVEL_TEXT, windowName } from "@agent-dashboard/app/windows";

/** The server's answers that the status bar item and the tree are made from. */
export interface Summary {
  status: Status;
  choices: Choices;
  /** Today's requests. */
  today: Overview;
  /** This week's requests, from its first day to today. */
  week: Overview;
  /** Today's and this week's requests of each agent, while the history holds more than one; the
   * tree then lists each agent's apart, since each may be paid for by someone else. */
  todayByAgent: AgentOverview[];
  weekByAgent: AgentOverview[];
  /** Whether Claude Code's status line records plan limits, as far as the settings tell. */
  limitsConnected: boolean;
}

/** A line of the status bar item's tooltip. */
export interface Line {
  label: string;
  value: string;
}

export interface StatusItem {
  /** The text after the gauge: `5h 42% · 14:20`, or `$5.05 today`. */
  text: string;
  /** What a screen reader says for the item. */
  label: string;
  /** How close the window the item shows is to its limit. */
  level: Level;
  lines: Line[];
  /** Where the numbers come from, under the lines. */
  note: string;
}

/** The icon of a tree item. */
export type Icon =
  | { kind: "gauge" }
  | { kind: "level"; level: "warning" | "critical" }
  | { kind: "folder" }
  | { kind: "model"; series: Series }
  | { kind: "connect" };

export interface Node {
  id: string;
  label: string;
  description: string;
  icon?: Icon;
  /** The section of the dashboard the item opens. */
  show: Show;
  children: Node[];
  /** Whether its children show at first. */
  expanded: boolean;
}

/** The most projects the tree lists, and the most of each agent's while it lists them apart. */
const TOP_PROJECTS = 5;
const TOP_PROJECTS_EACH = 3;

/** The newest reading among `readings`. */
function newest(readings: readonly WindowReading[]): WindowReading | undefined {
  return readings.reduce<WindowReading | undefined>(
    (best, reading) => (best && best.observedMs >= reading.observedMs ? best : reading),
    undefined,
  );
}

/** A cost as the dashboard writes it, saying when some requests have no price. */
function cost(totals: Totals): string {
  return totals.unpricedRequests > 0
    ? `${money(totals.cost.total)} + unpriced`
    : money(totals.cost.total);
}

/** Whether a window has reset since its reading, so that the reading no longer tells its use. The
 * server says so when it answers; between answers, the reset time does. */
function expired(window: WindowReading, nowMs: number): boolean {
  return window.expired || (window.resetsAtMs != null && window.resetsAtMs <= nowMs);
}

/** How much of a window is used and when it resets: `42% · resets 14:20`. With `marked`, a window
 * over 70% has `▲` after its percentage, as the tooltip shows it: `71% ▲ · resets Thu 09:00`. */
function reading(window: WindowReading, nowMs: number, timeZone: string, marked = false): string {
  const resetsAt = window.resetsAtMs;
  if (expired(window, nowMs)) {
    return resetsAt == null ? "reset" : `reset at ${when(resetsAt, nowMs, timeZone)}`;
  }
  const share = percent(window.usedPercentage);
  const used = marked && window.level !== "normal" ? `${share} ▲` : share;
  const credits = creditsText(window);
  if (credits) return `${used} · ${credits}`;
  return resetsAt == null ? used : `${used} · resets ${when(resetsAt, nowMs, timeZone)}`;
}

/** A window's level in words, for a sentence, as the dashboard writes it beside its mark. */
function levelWords(level: Level): string | undefined {
  return LEVEL_TEXT[level]?.toLowerCase();
}

/** Where a reading comes from, for a sentence: `official, status line`. */
function source(window: WindowReading): string {
  if (window.provenance === "official") return "official, status line";
  if (window.provenance === "recorded") return "recorded, Copilot's session files";
  return "experimental, usage endpoint";
}

/** What a cost is, after its figure: `$5.10 · estimate`, `$1.74 · recorded`, `$6.84 · includes
 * estimate`. An agent's cost without any priced request is what that agent's costs always are:
 * recorded for a Copilot agent, an estimate for Claude Code. */
function kind(totals: Totals, agent?: string): string {
  if (totals.costProvenance === "recorded") return "recorded";
  if (totals.costProvenance === "mixed") return "includes estimate";
  if (!totals.costProvenance && agent !== undefined && recordsCharges(agent)) return "recorded";
  return "estimate";
}

/** What today's cost is, for a sentence, by where it comes from. */
function costWords(totals: Totals): string {
  if (totals.costProvenance === "recorded") return "as Copilot recorded it";
  if (totals.costProvenance === "mixed") return "includes an API-equivalent estimate";
  return "API-equivalent estimate";
}

/** The status bar item: the five-hour window and Copilot's allowance while they have current
 * readings, and otherwise today's cost. */
export function statusItem(summary: Summary, nowMs: number): StatusItem {
  const { status } = summary;
  const windows = inOrder(status.limits);
  const lines = windows.map((window) => ({
    label: windowName(window),
    value: reading(window, nowMs, status.timeZone, true),
  }));
  lines.push({ label: "Today", value: cost(status.todayTotals) });
  const latest = PLANS.map((plan) =>
    newest(windows.filter((window) => window.plan === plan)),
  ).filter((window): window is WindowReading => window !== undefined);
  const costNote = `Cost: ${costWords(status.todayTotals)}.`;
  const note =
    latest.length > 0
      ? `Limits: ${latest.map((window) => `${source(window)}, ${ago(window.observedMs, nowMs)}`).join("; ")}. ${costNote}`
      : `${costNote} Plan limits are not recorded yet.`;
  const current = (name: string) =>
    windows.find((window) => window.window === name && !expired(window, nowMs));
  const fiveHour = current("five_hour");
  const allowance = current("monthly");
  if (fiveHour ?? allowance) {
    const texts: string[] = [];
    const spoken: string[] = [];
    if (fiveHour) {
      const used = percent(fiveHour.usedPercentage);
      const resetsAt = fiveHour.resetsAtMs;
      const time = resetsAt == null ? undefined : when(resetsAt, nowMs, status.timeZone);
      const words = levelWords(fiveHour.level);
      texts.push(time ? `5h ${used} · ${time}` : `5h ${used}`);
      spoken.push(
        `Five-hour window ${used} used${words ? `, ${words}` : ""}${time ? `, resets at ${time}` : ""}`,
      );
    }
    if (allowance) {
      const used = percent(allowance.usedPercentage);
      const words = levelWords(allowance.level);
      texts.push(`Copilot ${used}`);
      spoken.push(`Copilot allowance ${used} used${words ? `, ${words}` : ""}`);
    }
    const levels = [fiveHour, allowance]
      .filter((window): window is WindowReading => window !== undefined)
      .map((window) => window.level);
    return {
      text: texts.join(" · "),
      label: `${spoken.join("; ")}. Open the usage dashboard.`,
      level: levels.includes("critical")
        ? "critical"
        : levels.includes("warning")
          ? "warning"
          : "normal",
      lines,
      note,
    };
  }
  const today = cost(status.todayTotals);
  return {
    text: `${today} today`,
    label: `Cost today ${today}, ${costWords(status.todayTotals)}. Open the usage dashboard.`,
    level: "normal",
    lines,
    note,
  };
}

/** What a screen reader says for a tree item: its label and description, and in words the level
 * its icon shows. */
export function spoken(node: Node): string {
  const level = node.icon?.kind === "level" ? levelWords(node.icon.level) : undefined;
  return [node.label, node.description, level].filter(Boolean).join(", ");
}

/** The sidebar tree: plan limits, today by model, this week, and this week's top projects. */
export function tree(summary: Summary, nowMs: number): Node[] {
  const { status, today, week } = summary;
  const series = seriesOf(summary.choices.models.map((model) => model.id));
  const windows = inOrder(status.limits);
  const latest = newest(windows);
  const limits: Node = {
    id: "limits",
    label: "Plan limits",
    description: latest
      ? `${latest.provenance} · ${ago(latest.observedMs, nowMs)}`
      : summary.limitsConnected
        ? "waiting for Claude Code"
        : "not recorded",
    show: { section: "limits" },
    expanded: true,
    children: windows.map((window) => ({
      id: `limits/${window.plan}/${window.window}`,
      label: windowName(window),
      description: reading(window, nowMs, status.timeZone),
      icon:
        window.level === "normal" || expired(window, nowMs)
          ? { kind: "gauge" }
          : { kind: "level", level: window.level },
      show: { section: "limits" },
      children: [],
      expanded: false,
    })),
  };
  if (windows.length === 0 && !summary.limitsConnected) {
    limits.children.push({
      id: "limits/connect",
      label: "Connect Claude Code's status line",
      description: "",
      icon: { kind: "connect" },
      show: { section: "limits" },
      children: [],
      expanded: false,
    });
  }
  const todayOnly = { range: "today" } as const;
  /** A model's item: under its agent's item without a swatch, as the dashboard's tables show it. */
  const modelNode = (id: string, row: ModelRow, show: Show, agent?: string): Node => ({
    id: `${id}/${row.model.id}`,
    label: agent ? nameUnder(agent, row.model.name) : row.model.name,
    description: cost(row.totals),
    ...(agent ? {} : { icon: { kind: "model", series: series.get(row.model.id) ?? "other" } }),
    show,
    children: [],
    expanded: false,
  });
  /** Each agent's item, with its cost and its models' items under it. */
  const agentNodes = (
    id: string,
    groups: readonly AgentOverview[],
    range: Pick<Show, "range">,
    expanded: boolean,
  ): Node[] =>
    groups.map(({ agent, overview }) => ({
      id: `${id}/${agent}`,
      label: agentName(agent),
      description: `${cost(overview.totals)} · ${kind(overview.totals, agent)}`,
      icon: { kind: "model", series: agentSeries(agent) },
      show: { section: "overview", agent, ...range },
      expanded,
      children: overview.models.map((row) =>
        modelNode(`${id}/${agent}`, row, { section: "models", agent, ...range }, agent),
      ),
    }));
  const todayNode: Node = {
    id: "today",
    label: "Today",
    description: `${cost(status.todayTotals)} · ${kind(status.todayTotals)}`,
    show: { section: "overview", ...todayOnly },
    expanded: true,
    children:
      summary.todayByAgent.length > 0
        ? agentNodes("today", summary.todayByAgent, todayOnly, true)
        : today.models.map((row) => modelNode("today", row, { section: "models", ...todayOnly })),
  };
  const weekNode: Node = {
    id: "week",
    label: "This week",
    description: `${cost(status.weekTotals)} · ${kind(status.weekTotals)}`,
    show: { section: "overview" },
    expanded: false,
    children:
      summary.weekByAgent.length > 0
        ? agentNodes("week", summary.weekByAgent, {}, true)
        : week.models.map((row) => modelNode("week", row, { section: "models" })),
  };
  const projectNodes = (
    id: string,
    rows: Overview["projects"],
    count: number,
    agent?: string,
  ): Node[] =>
    rows.slice(0, count).map((row) => ({
      id: `${id}/${row.project.id}`,
      label: row.project.name,
      description: cost(row.totals),
      icon: { kind: "folder" },
      show: { section: "sessions", project: row.project.id, ...(agent ? { agent } : {}) },
      children: [],
      expanded: false,
    }));
  const projects: Node = {
    id: "projects",
    label: "Top projects",
    description: "this week",
    show: { section: "projects" },
    expanded: true,
    children:
      summary.weekByAgent.length > 0
        ? summary.weekByAgent.map(({ agent, overview }) => ({
            id: `projects/${agent}`,
            label: agentName(agent),
            description: cost(overview.totals),
            icon: { kind: "model", series: agentSeries(agent) },
            show: { section: "projects", agent },
            expanded: true,
            children: projectNodes(
              `projects/${agent}`,
              overview.projects,
              TOP_PROJECTS_EACH,
              agent,
            ),
          }))
        : projectNodes("projects", week.projects, TOP_PROJECTS),
  };
  return [limits, todayNode, weekNode, projects];
}
