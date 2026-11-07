// Overview: plan limits, today, this week and this month, cost per day by model, top projects and
// models, for the chosen days and filters.

import { useState } from "react";

import type {
  CostProvenance,
  Limits,
  ModelRow,
  Overview as OverviewAnswer,
  ProjectRow,
  Totals,
  WindowReading,
} from "@agent-dashboard/protocol";

import { agentName } from "../agents";
import { StackedColumns } from "../charts/columns";
import { Meter, ShareBar, Sparkline } from "../charts/small";
import { useHost, useStatus } from "../context";
import { useReadingProgress, useRequest } from "../data";
import {
  addDays,
  ago,
  compact,
  fullDate,
  grouped,
  money,
  percent,
  resets,
  startOfWeek,
} from "../format";
import { seriesFill } from "../models";
import { ChartIcon, TableIcon, Warning } from "../ui/icons";
import { Panel, Pause, Tile, TableScroll } from "../ui/panels";
import { useNow } from "../ui/time";
import { useView } from "../view";
import {
  AgentMenu,
  cacheShare,
  costName,
  costSource,
  costText,
  ModelMenu,
  ProjectMenu,
  RangeButtons,
  SectionHead,
  signedPercent,
  PROVENANCE_BADGE,
  tokensOf,
  useAgents,
  useDays,
  useFilters,
  useDaysScope,
  useProvenance,
  useRangeText,
  useSeries,
} from "./common";
import { ExportMenu } from "../exports";
import { StatusLineDialog } from "./dialogs";
import { Notices } from "./notices";
import { costColumns, costSeries, dailyCost } from "./series";
import { byAgent, creditsText, groupedName, LEVEL_TEXT, meterName, windowName } from "../windows";
import { LevelIcon, Provenance } from "./provenance";

export function Overview() {
  const host = useHost();
  const days = useDays();
  const filters = useFilters();
  const overview = useRequest("overview", { days, filters });
  const status = useStatus();
  const exportScope = useDaysScope();
  const progress = useReadingProgress() ?? status.reading;
  // With no history and nothing being read, there is nothing to show but how to find some.
  const nothingYet = !status.firstDay && !progress && status.limits.length === 0;
  return (
    <div className="section">
      <SectionHead title="Overview">
        <RangeButtons />
        <AgentMenu />
        <ProjectMenu />
        <ModelMenu />
        <ExportMenu
          what={{ dataset: "days", days, filters }}
          scope={exportScope}
          className="button export-button"
        />
      </SectionHead>
      <Notices />
      {nothingYet ? null : (
        <>
          {/* In VS Code's view the strip above shows the plan limits. */}
          {host.placement !== "view" && <PlanLimits />}
          <Tiles />
          <div className="row">
            <DailyCost overview={overview.data} />
            <TopProjects projects={overview.data?.projects} totals={overview.data?.totals} />
          </div>
          <ModelsPanel models={overview.data?.models} totals={overview.data?.totals} />
        </>
      )}
    </div>
  );
}

function PlanLimits() {
  const status = useStatus();
  const now = useNow(30_000);
  const limits = useRequest("limits", null).data;
  const agents = useRequest("choices", null).data?.agents.map((agent) => agent.id);
  const groups = byAgent(status.limits);
  if (groups.length === 0) {
    if (agents?.length === 1 && agents[0] === "copilot-cli") return <NoAllowance />;
    return <NoLimits limits={limits} />;
  }
  // Claude Code's windows need its status line: while the history holds Claude Code's requests
  // but no reading, its group says how to get one.
  const claudeWaits =
    agents?.includes("claude-code") === true &&
    !groups.some((group) => group.agent === "claude-code");
  const shown = claudeWaits ? [{ agent: "claude-code", readings: [] }, ...groups] : groups;
  const [only] = shown;
  if (shown.length === 1 && only) {
    return (
      <Panel title="Plan limits" actions={<Provenance readings={only.readings} nowMs={now} />}>
        <Meters readings={only.readings} nowMs={now} name={windowName} />
        <GroupNote agent={only.agent} readings={only.readings} limits={limits} nowMs={now} />
      </Panel>
    );
  }
  return (
    <Panel title="Plan limits">
      <div className="limit-groups">
        {shown.map((group) => (
          <section
            key={group.agent}
            className="limit-group"
            aria-label={`${agentName(group.agent)}'s plan limits`}
          >
            <div className="limit-group-head">
              <h3>{agentName(group.agent)}</h3>
              <Provenance readings={group.readings} nowMs={now} />
            </div>
            {group.readings.length > 0 ? (
              <>
                <Meters readings={group.readings} nowMs={now} name={groupedName} />
                <GroupNote
                  agent={group.agent}
                  readings={group.readings}
                  limits={limits}
                  nowMs={now}
                />
              </>
            ) : (
              <ConnectStatusLine limits={limits} />
            )}
          </section>
        ))}
      </div>
    </Panel>
  );
}

/** A meter for each window: its use, its level and when it resets. */
function Meters({
  readings,
  nowMs,
  name,
}: {
  readings: readonly WindowReading[];
  nowMs: number;
  name: (reading: WindowReading) => string;
}) {
  const status = useStatus();
  return (
    <div className="meters">
      {readings.map((reading) => {
        const credits = reading.expired ? undefined : creditsText(reading);
        return (
          <div key={`${reading.agent}:${reading.window}`} className="meter-block">
            <div className="meter-head">
              <span className="meter-name">{name(reading)}</span>
              <span
                className={reading.stale || reading.expired ? "meter-figure stale" : "meter-figure"}
              >
                {reading.expired ? "Reset" : percent(reading.usedPercentage)}
              </span>
            </div>
            <Meter
              used={reading.expired ? 0 : reading.usedPercentage}
              level={reading.level}
              stale={reading.stale}
              label={`${meterName(reading)}, ${percent(reading.expired ? 0 : reading.usedPercentage)} used`}
            />
            <div className="meter-foot">
              {!reading.expired && LEVEL_TEXT[reading.level] && (
                <>
                  <span className="level">
                    <LevelIcon level={reading.level} />
                    {LEVEL_TEXT[reading.level]}
                  </span>
                  <span aria-hidden="true">·</span>
                </>
              )}
              {credits && (
                <>
                  <span>{credits}</span>
                  <span aria-hidden="true">·</span>
                </>
              )}
              <span>
                {reading.expired
                  ? "No reading since it reset"
                  : reading.resetsAtMs
                    ? resets(reading.resetsAtMs, nowMs, status.timeZone)
                    : "No reset time"}
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );
}

/** What an agent's readings need said: why they are old, or what Copilot's allowance counts. */
function GroupNote({
  agent,
  readings,
  limits,
  nowMs,
}: {
  agent: string;
  readings: readonly WindowReading[];
  limits: Limits | undefined;
  nowMs: number;
}) {
  const stale = readings.every((reading) => reading.stale || reading.expired);
  if (agent === "copilot-cli") {
    const newest = Math.max(...readings.map((reading) => reading.observedMs));
    return (
      <p className="note">
        {stale
          ? `Copilot CLI last noted the allowance ${ago(newest, nowMs)}; it updates the next time Copilot CLI runs. `
          : ""}
        The allowance counts Copilot wherever you use it, not just the CLI
        {stale ? "." : ", and updates while Copilot CLI runs."}
      </p>
    );
  }
  return stale ? <StaleNote limits={limits} readings={readings} /> : null;
}

/** Why Claude Code's readings are old, and what brings newer ones. */
function StaleNote({
  limits,
  readings,
}: {
  limits: Limits | undefined;
  readings: readonly WindowReading[];
}) {
  const status = useStatus();
  const newest = Math.max(...readings.map((reading) => reading.observedMs));
  const at = new Intl.DateTimeFormat("en-GB", {
    timeZone: status.timeZone,
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).format(newest);
  if (limits?.endpoint.enabled && limits.endpoint.outcome === "login-expired") {
    return (
      <p className="note">
        Claude Code's login on this computer has expired, so the usage endpoint can't be asked. Open
        Claude Code once to sign in again; agent-dashboard never refreshes the login itself.
      </p>
    );
  }
  return (
    <p className="note">
      Claude Code hasn't run in a terminal since {at}, so there is no newer reading. Use Claude Code
      in a terminal
      {limits?.endpoint.enabled
        ? "."
        : ", or turn on the experimental usage endpoint to fill the gaps."}
    </p>
  );
}

function NoLimits({ limits }: { limits: Limits | undefined }) {
  if (!limits) return null;
  return (
    <Panel title="Plan limits">
      <ConnectStatusLine limits={limits} />
    </Panel>
  );
}

/** Copilot CLI's allowance before Copilot CLI has noted it. */
function NoAllowance() {
  return (
    <Panel title="Plan limits">
      <p className="note">
        Your Copilot allowance appears here once Copilot CLI makes a request. It counts Copilot
        wherever you use it, not just the CLI.
      </p>
    </Panel>
  );
}

/** How Claude Code's windows get here: its status line. */
function ConnectStatusLine({ limits }: { limits: Limits | undefined }) {
  const { change } = useView();
  const [connecting, setConnecting] = useState(false);
  if (!limits) return null;
  if (limits.statusLine.state === "connected") {
    return (
      <p className="note">
        Claude Code's status line is connected. Your five-hour and weekly windows appear here the
        next time Claude Code runs in a terminal.
      </p>
    );
  }
  return (
    <>
      <p className="note">
        Connect Claude Code's status line to see your five-hour and weekly windows. It is the
        official source and updates while Claude Code runs in a terminal.
      </p>
      <div className="actions">
        <button
          type="button"
          className="button primary"
          onClick={() => {
            setConnecting(true);
          }}
        >
          Connect the status line…
        </button>
        <button
          type="button"
          className="button quiet"
          onClick={() => {
            change({ section: "limits" });
          }}
        >
          Other options
        </button>
      </div>
      {connecting && (
        <StatusLineDialog
          connect
          onClose={() => {
            setConnecting(false);
          }}
        />
      )}
    </>
  );
}

/** Today, this week and this month, with the days before each for comparison. */
function Tiles() {
  const status = useStatus();
  const filters = useFilters();
  const agents = useAgents();
  const today = status.today;
  const weekStart = startOfWeek(today, status.weekStart);
  const monthStart = `${today.slice(0, 8)}01`;
  const first = [addDays(today, -29), monthStart, addDays(weekStart, -7)].sort()[0] ?? today;
  const recent = useRequest("overview", { days: { first, last: today }, filters }).data;
  if (!recent) return <div className="tiles" />;
  const sum = (from: string, to: string) => {
    let cost = 0;
    let tokens = 0;
    let chargedUsd = 0;
    let chargedCredits = 0;
    const kinds = new Set<CostProvenance>();
    for (const day of recent.byDay) {
      if (day.date < from || day.date > to) continue;
      cost += day.totals.cost.total;
      tokens += tokensOf(day.totals);
      chargedUsd += day.totals.chargedUsd;
      chargedCredits += day.totals.chargedCredits;
      if (day.totals.costProvenance) kinds.add(day.totals.costProvenance);
    }
    return { cost, tokens, chargedUsd, chargedCredits, provenance: combined(kinds) };
  };
  const todays = sum(today, today);
  const yesterdays = sum(addDays(today, -1), addDays(today, -1));
  const week = sum(weekStart, today);
  const lastWeek = sum(addDays(weekStart, -7), addDays(today, -7));
  const month = sum(monthStart, today);
  const spark = (count: number) => dailyCost(recent.byDay, addDays(today, 1 - count), today);
  const compared = (now: number, before: number, what: string) => {
    const change = signedPercent(now, before);
    return change ? `${change} on ${what}` : `nothing ${what}`;
  };
  // Claude Code's cost is all estimate and Copilot CLI's all recorded charges, so the two parts
  // of a figure are its agents' shares.
  const split = agents.length > 1;
  const parts = (period: ReturnType<typeof sum>) =>
    split ? (
      <dl className="tile-breakdown">
        <dt>Claude Code</dt>
        <dd>
          {money(Math.max(0, period.cost - period.chargedUsd))}{" "}
          <span className="muted">estimate</span>
        </dd>
        <dt>Copilot CLI</dt>
        <dd>
          {money(period.chargedUsd)}{" "}
          <span className="muted">
            recorded · {grouped(period.chargedCredits)}{" "}
            {Math.round(period.chargedCredits) === 1 ? "credit" : "credits"}
          </span>
        </dd>
      </dl>
    ) : undefined;
  const badge = (period: ReturnType<typeof sum>) =>
    period.provenance ? (
      <span className="badge">{PROVENANCE_BADGE[period.provenance]}</span>
    ) : undefined;
  return (
    <div className="tiles">
      <Tile
        label="Today"
        value={money(todays.cost)}
        note={`${compact(todays.tokens)} tokens · ${compared(todays.cost, yesterdays.cost, "yesterday")}`}
        badge={badge(todays)}
        breakdown={parts(todays)}
      >
        <Sparkline values={spark(12)} label="Daily cost over the last 12 days" format={money} />
      </Tile>
      <Tile
        label="This week"
        value={money(week.cost)}
        note={`${compact(week.tokens)} tokens · ${compared(week.cost, lastWeek.cost, "last week")}`}
        badge={badge(week)}
        breakdown={parts(week)}
      >
        <Sparkline values={spark(18)} label="Daily cost over the last 18 days" format={money} />
      </Tile>
      <Tile
        label="This month"
        value={money(month.cost)}
        note={`${compact(month.tokens)} tokens · month to date`}
        badge={badge(month)}
        breakdown={parts(month)}
      >
        <Sparkline values={spark(30)} label="Daily cost over the last 30 days" format={money} />
      </Tile>
    </div>
  );
}

/** Where the costs of several days come from together. */
function combined(kinds: ReadonlySet<CostProvenance>): CostProvenance | undefined {
  if (kinds.has("mixed") || (kinds.has("estimate") && kinds.has("recorded"))) return "mixed";
  if (kinds.has("recorded")) return "recorded";
  if (kinds.has("estimate")) return "estimate";
  return undefined;
}

function DailyCost({ overview }: { overview: OverviewAnswer | undefined }) {
  const status = useStatus();
  const series = useSeries();
  const rangeText = useRangeText();
  const [asTable, setAsTable] = useState(false);
  const provenance = useProvenance(overview?.totals);
  const name = costName(provenance);
  if (!overview) return <section className="panel grow" aria-label={`${name} per day`} />;
  const chart = costSeries(overview.models, series);
  const { bucket, columns } = costColumns(
    overview.byDay,
    chart,
    overview.days.first,
    overview.days.last,
    status.weekStart,
  );
  const per = bucket === "day" ? "day" : bucket === "week" ? "week" : "month";
  const totals = columns.map((column) => [...column.values.values()].reduce((a, b) => a + b, 0));
  const highest = totals.reduce(
    (best, total, index) => (total > (totals[best] ?? 0) ? index : best),
    0,
  );
  const label = `${name} per ${per} by model, ${rangeText}: ${money(
    overview.totals.cost.total,
  )} in all${
    columns[highest] && (totals[highest] ?? 0) > 0
      ? `, highest ${money(totals[highest] ?? 0)} on ${columns[highest].title}`
      : ""
  }.`;
  return (
    <Panel
      className="grow"
      title={`${name} per ${per}`}
      subtitle={`${costSource(provenance)} · ${rangeText}`}
      actions={
        <button
          type="button"
          className="button small quiet"
          onClick={() => {
            setAsTable(!asTable);
          }}
        >
          {asTable ? <ChartIcon /> : <TableIcon />}
          {asTable ? "Show as chart" : "Show as table"}
        </button>
      }
    >
      {chart.series.length > 1 && (
        <ul aria-label="Legend" className="legend">
          {chart.series.map((item) => (
            <li key={item.id}>
              <span className="swatch" style={{ background: item.fill }} />
              <span>{item.name}</span>
              <Pause />
              <span className="figure strong">{money(item.total)}</span>
            </li>
          ))}
        </ul>
      )}
      {asTable ? (
        <TableScroll label="Cost per day by model">
          <table className="data">
            <thead>
              <tr>
                <th scope="col">{per === "day" ? "Day" : per === "week" ? "Week" : "Month"}</th>
                {chart.series.map((item) => (
                  <th key={item.id} scope="col">
                    {item.name}
                  </th>
                ))}
                <th scope="col">Total</th>
              </tr>
            </thead>
            <tbody>
              {columns.map((column, index) => (
                <tr key={column.key}>
                  <th scope="row">{column.title}</th>
                  {chart.series.map((item) => (
                    <td key={item.id}>{money(column.values.get(item.id) ?? 0)}</td>
                  ))}
                  <td className="strong">{money(totals[index] ?? 0)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </TableScroll>
      ) : (
        <StackedColumns columns={columns} series={chart.series} label={label} format={money} />
      )}
    </Panel>
  );
}

/** The projects that cost the most, and the rest together. */
function TopProjects({
  projects,
  totals,
}: {
  projects: readonly ProjectRow[] | undefined;
  totals: Totals | undefined;
}) {
  const rangeText = useRangeText();
  const provenance = useProvenance(totals);
  const rows = projects ?? [];
  const shown = rows.length > 6 ? rows.slice(0, 5) : rows;
  const rest = rows.slice(shown.length);
  const items = [
    ...shown.map((row) => ({
      key: row.project.id,
      name: row.project.name,
      cost: row.totals.cost.total,
      mono: true,
    })),
    ...(rest.length > 0
      ? [
          {
            key: "other",
            name: "Other projects",
            cost: rest.reduce((sum, row) => sum + row.totals.cost.total, 0),
            mono: false,
          },
        ]
      : []),
  ];
  const largest = Math.max(...items.map((item) => item.cost), 0);
  return (
    <Panel
      className="side"
      title="Top projects"
      subtitle={`${costName(provenance)} · ${rangeText}`}
    >
      {projects?.length === 0 ? (
        <p className="note">
          No projects {rangeText === "today" ? "today" : `in the ${rangeText}`}.
        </p>
      ) : (
        <ol className="ranking">
          {items.map((item) => (
            <li key={item.key}>
              <div className="ranking-row">
                <span className={item.mono ? "mono name" : "name"}>{item.name}</span>
                <Pause />
                <span className="figure">{money(item.cost)}</span>
              </div>
              <ShareBar share={largest > 0 ? item.cost / largest : 0} />
            </li>
          ))}
        </ol>
      )}
    </Panel>
  );
}

function ModelsPanel({
  models,
  totals,
}: {
  models: readonly ModelRow[] | undefined;
  totals: OverviewAnswer["totals"] | undefined;
}) {
  const rangeText = useRangeText();
  const prices = useRequest("prices", { models: [] }).data;
  const series = useSeries();
  const provenance = useProvenance(totals);
  return (
    <Panel
      title="Models"
      aside={`· ${rangeText}`}
      actions={
        totals && tokensOf(totals) > 0 ? (
          <span className="subtitle">
            {percent(cacheShare(totals) * 100)} of input read from cache
          </span>
        ) : undefined
      }
    >
      <TableScroll label="Models">
        <table className="data wide">
          <thead>
            <tr>
              <th scope="col">Model</th>
              <th scope="col">Requests</th>
              <th scope="col">Input</th>
              <th scope="col">Output</th>
              <th scope="col">Cache writes</th>
              <th scope="col">Cache reads</th>
              <th scope="col">{costName(provenance)}</th>
            </tr>
          </thead>
          <tbody>
            {(models ?? []).map((row) => {
              const unpriced =
                row.totals.requests > 0 && row.totals.unpricedRequests === row.totals.requests;
              return (
                <tr key={row.model.id}>
                  <th scope="row" className="strong">
                    <span className="swatch-name">
                      <span
                        className="swatch"
                        style={{ background: seriesFill(series.get(row.model.id) ?? "other") }}
                      />
                      <span className={unpriced ? "mono plain" : undefined}>{row.model.name}</span>
                    </span>
                  </th>
                  <td>{grouped(row.totals.requests)}</td>
                  <td>{compact(row.totals.tokens.input)}</td>
                  <td>{compact(row.totals.tokens.output)}</td>
                  <td>
                    {compact(row.totals.tokens.cacheWrite5m + row.totals.tokens.cacheWrite1h)}
                  </td>
                  <td>{compact(row.totals.tokens.cacheRead)}</td>
                  <td className="strong">
                    {unpriced ? (
                      <span className="unpriced">
                        <Warning size={13} />
                        Unpriced
                      </span>
                    ) : (
                      costText(row.totals)
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </TableScroll>
      {provenance !== "estimate" && (
        <div className="estimate">
          <span className="badge">Recorded</span>
          <span>Copilot CLI's charges, at $0.01 an AI credit</span>
        </div>
      )}
      {prices && provenance !== "recorded" && (
        <div className="estimate">
          <span className="badge">Estimate</span>
          <span>
            Price table of {fullDate(prices.checked)}
            {prices.drift !== null && prices.drift !== undefined && prices.outOfLine.length === 0
              ? ` · within ${Math.max(0.1, Math.abs(prices.drift) * 100).toFixed(1)}% of Claude Code's own session costs`
              : ""}
          </span>
        </div>
      )}
    </Panel>
  );
}
