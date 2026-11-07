// Sessions: every session in the chosen days, most recent first, a page at a time; each opens its
// detail.

import type { MouseEvent } from "react";

import type { Model, SessionRow } from "@agent-dashboard/protocol";

import { useStatus } from "../context";
import { useRequest } from "../data";
import { agentName } from "../agents";
import { compact, dayAndTime, duration, grouped, money } from "../format";
import { seriesFill } from "../models";
import { isRunning } from "../sessions";
import { SearchField } from "../ui/controls";
import { Check, ChevronDown, Warning } from "../ui/icons";
import { Pill, TableScroll } from "../ui/panels";
import { toTop } from "../ui/scroll";
import { useNow } from "../ui/time";
import { useView } from "../view";
import {
  AgentMenu,
  COST_WORDS,
  costName,
  costText,
  ModelMenu,
  ProjectMenu,
  RangeMenu,
  SectionHead,
  tokensOf,
  useAgents,
  useDays,
  useFilters,
  useExportScope,
  useProvenance,
  useRangeText,
  useSeries,
} from "./common";
import { ExportMenu } from "../exports";
import { SessionDetail } from "./SessionDetail";

/** Sessions on one page. */
const PAGE = 12;

/** How far the estimate may be from Claude Code's figure before prices may be out of date. */
const DRIFT_LIMIT = 0.02;

export function Sessions() {
  const { view } = useView();
  if (view.session !== null) return <SessionDetail id={view.session} />;
  return <SessionList />;
}

function SessionList() {
  const { view, change } = useView();
  const days = useDays();
  const filters = useFilters();
  const rangeText = useRangeText();
  const page = useRequest("sessions", {
    days,
    filters,
    search: view.search === "" ? null : view.search,
    offset: view.offset,
    limit: PAGE,
  }).data;
  const overview = useRequest("overview", { days, filters }).data;
  const provenance = useProvenance(overview?.totals);
  const agents = useAgents();
  // With both agents in view, each row says whose session it is; Claude Code's own figure applies
  // to Claude Code's sessions only.
  const both = agents.length > 1;
  const claude = agents.includes("claude-code");
  const summary =
    page && overview && view.search === ""
      ? `${grouped(page.total)} ${page.total === 1 ? "session" : "sessions"} ${
          rangeText === "today" ? "today" : `in the ${rangeText}`
        } · ${money(overview.totals.cost.total)} ${COST_WORDS[provenance]}`
      : page
        ? `${grouped(page.total)} ${page.total === 1 ? "session matches" : "sessions match"}`
        : undefined;
  const total = page?.total ?? 0;
  const exportScope = useExportScope(
    total,
    "session",
    "sessions",
    total > PAGE ? "that match, not only this page" : "that match",
  );
  return (
    <div className="section">
      <SectionHead title="Sessions" summary={summary}>
        <SearchField
          label="Search sessions"
          placeholder="Search titles"
          value={view.search}
          onChange={(search) => {
            change({ search });
          }}
        />
        <RangeMenu />
        <AgentMenu />
        <ProjectMenu />
        <ModelMenu />
        <ExportMenu
          what={{
            dataset: "sessions",
            days,
            filters,
            search: view.search === "" ? null : view.search,
          }}
          scope={exportScope}
          className="button export-button"
        />
      </SectionHead>
      <section aria-label="Sessions" className="panel list">
        <TableScroll label="Sessions">
          <table className="data sessions">
            <thead>
              <tr>
                <th scope="col">Session</th>
                {both && (
                  <th scope="col" className="left">
                    Agent
                  </th>
                )}
                <th scope="col" aria-sort="descending">
                  <span className="sorted">
                    Started
                    <ChevronDown size={12} stroke={2.25} />
                  </span>
                </th>
                <th scope="col">Duration</th>
                <th scope="col" className="left">
                  Models
                </th>
                <th scope="col">Requests</th>
                <th scope="col">Subagents</th>
                <th scope="col">Tokens</th>
                <th scope="col">{costName(provenance)}</th>
                {claude && (
                  <th scope="col" className="left">
                    Claude Code's own figure
                  </th>
                )}
              </tr>
            </thead>
            <tbody>
              {page?.rows.map((row) => (
                <Row key={row.id} row={row} agent={both} ownFigure={claude} />
              ))}
            </tbody>
          </table>
        </TableScroll>
        {page?.total === 0 && (
          <p className="note empty-list">
            {view.search === "" ? "No sessions in these days." : "No session matches the search."}
          </p>
        )}
        {page && page.total > 0 && (
          <div className="pages">
            <span>
              Showing {grouped(view.offset + 1)} to{" "}
              {grouped(Math.min(page.total, view.offset + page.rows.length))} of{" "}
              {grouped(page.total)} sessions
            </span>
            <div className="actions">
              <button
                type="button"
                className="button small quiet"
                disabled={view.offset === 0}
                onClick={() => {
                  change({ offset: Math.max(0, view.offset - PAGE) });
                }}
              >
                Previous
              </button>
              <button
                type="button"
                className="button small raised"
                disabled={view.offset + PAGE >= page.total}
                onClick={() => {
                  change({ offset: view.offset + PAGE });
                }}
              >
                Next
              </button>
            </div>
          </div>
        )}
      </section>
      {claude && (
        <p className="footnote">
          "Claude Code's own figure" compares our estimate with the session cost Claude Code records
          itself, which it writes as each of its processes ends. A figure that counts other requests
          than the history holds is not compared.
          {both &&
            " Copilot CLI's sessions cost what Copilot CLI recorded, so there is nothing to compare."}
        </p>
      )}
    </div>
  );
}

function Row({
  row,
  agent,
  ownFigure,
}: {
  row: SessionRow;
  /** Shows whose session it is, which also tells whether its cost is an estimate or recorded. */
  agent: boolean;
  /** Shows how the estimate compares with Claude Code's own figure. */
  ownFigure: boolean;
}) {
  const status = useStatus();
  const { change } = useView();
  const now = useNow(60_000);
  const series = useSeries();
  const running = isRunning(row, now);
  const open = (event: MouseEvent) => {
    event.preventDefault();
    change({ session: row.id });
    toTop();
  };
  const priced = row.models.filter((model) => !unpriced(model));
  const unpricedCount = row.models.length - priced.length;
  return (
    <tr>
      <th scope="row" className="session-cell">
        <a href={`#session-${row.id}`} onClick={open} className="session-link">
          <span className="session-title">
            <span className="title-text">{row.title ?? "Untitled session"}</span>
            {running && <Pill tone="accent">Running</Pill>}
          </span>
          <span className="mono session-project">{row.project.name}</span>
        </a>
      </th>
      {agent && <td className="left nowrap">{agentName(row.agent)}</td>}
      <td className="nowrap">{dayAndTime(row.firstMs, now, status.timeZone)}</td>
      <td className="nowrap">{duration(row.lastMs - row.firstMs)}</td>
      <td className="left">
        <span
          className="model-swatches"
          role="img"
          aria-label={`${priced.map((model) => model.name).join(", ")}${
            unpricedCount > 0
              ? `${priced.length > 0 ? ", and " : ""}${unpricedCount === 1 ? "one unpriced model" : `${String(unpricedCount)} unpriced models`}`
              : ""
          }`}
        >
          {priced.map((model) => (
            <span
              key={model.id}
              className="swatch"
              style={{ background: seriesFill(series.get(model.id) ?? "other") }}
            />
          ))}
        </span>
      </td>
      <td>{grouped(row.totals.requests)}</td>
      <td>{grouped(row.subagents)}</td>
      <td>{compact(tokensOf(row.totals))}</td>
      <td className="strong nowrap">{costText(row.totals)}</td>
      {ownFigure && (
        <td className="left own-figure">
          {row.agent === "claude-code" ? (
            <OwnFigure
              reported={row.reportedUsd}
              drift={row.drift}
              running={running}
              unpricedModels={unpricedCount}
            />
          ) : (
            <span className="figure-check muted">Recorded cost</span>
          )}
        </td>
      )}
    </tr>
  );
}

/** A model the price table does not list: the server names it by its id. */
export function unpriced(model: Model): boolean {
  return model.name === model.id;
}

/** How the estimate compares with the cost Claude Code recorded for the session. */
export function OwnFigure({
  reported,
  drift,
  running,
  unpricedModels,
  long = false,
}: {
  reported: number | null | undefined;
  /** How far the estimate is from Claude Code's figure for the same requests, when it counts
   * them. */
  drift: number | null | undefined;
  running: boolean;
  unpricedModels: number;
  /** Says `Matches within 0.1%` rather than `Within 0.1%`. */
  long?: boolean;
}) {
  if (unpricedModels > 0) {
    return (
      <span className="figure-check strong-text">
        <Warning size={14} className="warning-mark" />
        {unpricedModels === 1 ? "1 model unpriced" : `${String(unpricedModels)} models unpriced`}
      </span>
    );
  }
  if (running) {
    return <span className="figure-check muted">Not ended yet</span>;
  }
  if (reported === null || reported === undefined || reported <= 0) {
    return <span className="figure-check muted">None recorded</span>;
  }
  if (drift === null || drift === undefined) {
    return (
      <span className="figure-check muted">
        {long ? "Counts other requests, so not compared" : "Not comparable"}
      </span>
    );
  }
  if (Math.abs(drift) <= DRIFT_LIMIT) {
    const within = Math.max(0.1, Math.abs(drift) * 100).toFixed(1);
    return (
      <span className="figure-check">
        <Check size={14} />
        {long ? `Matches within ${within}%` : `Within ${within}%`}
      </span>
    );
  }
  return (
    <span className="figure-check strong-text">
      <Warning size={14} className="warning-mark" />
      {String(Math.round(Math.abs(drift) * 100))}% {drift < 0 ? "lower" : "higher"}, prices may be
      out of date
    </span>
  );
}
