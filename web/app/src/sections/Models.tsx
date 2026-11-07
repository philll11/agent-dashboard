// Models: each model's cost per day, its tokens by kind and share of fast mode, and the prices the
// estimate uses, for the chosen days and project.

import { useState } from "react";

import type { CostProvenance, ModelRow, PriceRow } from "@agent-dashboard/protocol";

import { LineChart } from "../charts/lines";
import { moneyTick, niceScale } from "../charts/scale";
import { useStatus } from "../context";
import { useRequest } from "../data";
import { compact, fullDate, grouped, money, percent, price } from "../format";
import { seriesFill } from "../models";
import { ChartIcon, TableIcon, Warning } from "../ui/icons";
import { Panel, Pause, TableScroll } from "../ui/panels";
import {
  AgentMenu,
  COST_WORDS,
  costName,
  costSource,
  costText,
  ProjectMenu,
  RangeMenu,
  SectionHead,
  useAgents,
  useDays,
  useFilters,
  useExportScope,
  useProvenance,
  useRangeText,
  useSeries,
} from "./common";
import { ExportMenu } from "../exports";
import { costColumns, costSeries } from "./series";
import { unpriced } from "./Sessions";

export function Models() {
  const days = useDays();
  const filters = useFilters();
  const rangeText = useRangeText();
  const overview = useRequest("overview", { days, filters: { ...filters, model: null } }).data;
  const models = overview?.models ?? [];
  const provenance = useProvenance(overview?.totals);
  const summary = overview
    ? `${grouped(models.length)} ${models.length === 1 ? "model" : "models"} ${
        rangeText === "today" ? "today" : `in the ${rangeText}`
      } · ${money(overview.totals.cost.total)} ${COST_WORDS[provenance]}`
    : undefined;
  const exportScope = useExportScope(models.length, "model", "models");
  return (
    <div className="section">
      <SectionHead title="Models" summary={summary}>
        <RangeMenu />
        <AgentMenu />
        <ProjectMenu />
        <ExportMenu
          what={{ dataset: "models", days, filters: { ...filters, model: null } }}
          scope={exportScope}
          className="button export-button"
        />
      </SectionHead>
      <DailyLines />
      <TokensAndCost models={models} provenance={provenance} />
      <PricesUsed models={models} />
    </div>
  );
}

function DailyLines() {
  const status = useStatus();
  const days = useDays();
  const filters = useFilters();
  const rangeText = useRangeText();
  const series = useSeries();
  const [asTable, setAsTable] = useState(false);
  const overview = useRequest("overview", { days, filters: { ...filters, model: null } }).data;
  const provenance = useProvenance(overview?.totals);
  const name = costName(provenance);
  if (!overview) return <section className="panel" aria-label={`${name} per day, by model`} />;
  const chart = costSeries(overview.models, series);
  const { bucket, columns } = costColumns(
    overview.byDay,
    chart,
    overview.days.first,
    overview.days.last,
    status.weekStart,
  );
  const per = bucket === "day" ? "day" : bucket === "week" ? "week" : "month";
  const highest = Math.max(0, ...columns.flatMap((column) => [...column.values.values()]));
  const scale = niceScale(highest, 2);
  const last = Math.max(1, columns.length - 1);
  const labelEvery = columns.length > 20 ? 5 : columns.length > 10 ? 2 : 1;
  const xLabels = columns.flatMap((column, index) =>
    index % labelEvery === 0 || index === columns.length - 1
      ? [
          {
            x: index,
            text: column.title.replace(/^(Mon|Tue|Wed|Thu|Fri|Sat|Sun) |^Week of \w+ /, ""),
          },
        ]
      : [],
  );
  const label = `${chart.series.length === 1 ? "A line" : `${String(chart.series.length)} lines`} of ${name.toLowerCase()} per ${per}, ${rangeText}: ${chart.series
    .map((item) => `${item.name} ${money(item.total)}`)
    .join(", ")}.`;
  return (
    <Panel
      title={`${name} per ${per}, by model`}
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
              <span className="line-swatch" style={{ background: lineColour(item.series) }} />
              <span>{item.name}</span>
              <Pause />
              <span className="figure">{money(item.total)}</span>
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
              </tr>
            </thead>
            <tbody>
              {columns.map((column) => (
                <tr key={column.key}>
                  <th scope="row">{column.title}</th>
                  {chart.series.map((item) => (
                    <td key={item.id}>{money(column.values.get(item.id) ?? 0)}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </TableScroll>
      ) : (
        <LineChart
          lines={chart.series.map((item) => ({
            id: item.id,
            colour: lineColour(item.series),
            points: columns.map((column, index) => ({
              x: index,
              y: column.values.get(item.id) ?? 0,
            })),
          }))}
          x={[0, last]}
          yTop={scale.top}
          height={200}
          ticks={scale.ticks.map((tick) => ({ y: tick, text: moneyTick(tick, scale.ticks) }))}
          xLabels={xLabels}
          label={label}
          probe={(at) => {
            const index = Math.round(at);
            const column = columns[index];
            if (!column) return undefined;
            const values = chart.series.filter((item) => (column.values.get(item.id) ?? 0) > 0);
            return {
              x: index,
              title: column.title,
              rows: values.map((item) => (
                <div key={item.id} className="tooltip-row">
                  <span className="swatch-name">
                    <span className="line-swatch" style={{ background: lineColour(item.series) }} />
                    {item.name}
                  </span>
                  <span className="figure">{money(column.values.get(item.id) ?? 0)}</span>
                </div>
              )),
              summary: `${column.title}: ${values
                .map((item) => `${item.name} ${money(column.values.get(item.id) ?? 0)}`)
                .join(", ")}.`,
            };
          }}
        />
      )}
    </Panel>
  );
}

/** A line's colour: the series colour itself, since high contrast patterns are for areas. */
function lineColour(series: string): string {
  return series === "other" ? "var(--series-other)" : `var(--series-${series})`;
}

function TokensAndCost({
  models,
  provenance,
}: {
  models: readonly ModelRow[];
  provenance: CostProvenance;
}) {
  const rangeText = useRangeText();
  const series = useSeries();
  // Only Copilot CLI records reasoning tokens, which its output tokens already count.
  const reasoning = models.some((row) => row.totals.reasoningTokens > 0);
  return (
    <Panel title="Tokens and cost" aside={`· ${rangeText}`}>
      <TableScroll label="Tokens and cost">
        <table className="data wider">
          <thead>
            <tr>
              <th scope="col">Model</th>
              <th scope="col">Requests</th>
              <th scope="col">Input</th>
              <th scope="col">Output</th>
              {reasoning && <th scope="col">Of it reasoning</th>}
              <th scope="col">Cache writes, 5 min</th>
              <th scope="col">Cache writes, 1 h</th>
              <th scope="col">Cache reads</th>
              <th scope="col">Fast mode</th>
              <th scope="col">{costName(provenance)}</th>
            </tr>
          </thead>
          <tbody>
            {models.map((row) => {
              const isUnpriced = unpriced(row.model);
              return (
                <tr key={row.model.id}>
                  <th scope="row" className="strong">
                    <span className="swatch-name">
                      <span
                        className="swatch"
                        style={{ background: seriesFill(series.get(row.model.id) ?? "other") }}
                      />
                      <span className={isUnpriced ? "mono plain" : undefined}>
                        {row.model.name}
                      </span>
                    </span>
                  </th>
                  <td>{grouped(row.totals.requests)}</td>
                  <td>{compact(row.totals.tokens.input)}</td>
                  <td>{compact(row.totals.tokens.output)}</td>
                  {reasoning && <td>{compact(row.totals.reasoningTokens)}</td>}
                  <td>{compact(row.totals.tokens.cacheWrite5m)}</td>
                  <td>{compact(row.totals.tokens.cacheWrite1h)}</td>
                  <td>{compact(row.totals.tokens.cacheRead)}</td>
                  <td>
                    {percent(
                      row.totals.requests > 0
                        ? (row.totals.fastRequests / row.totals.requests) * 100
                        : 0,
                    )}
                  </td>
                  <td className="strong">
                    {isUnpriced ? (
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
    </Panel>
  );
}

function PricesUsed({ models }: { models: readonly ModelRow[] }) {
  const ids = models.filter((row) => !unpriced(row.model)).map((row) => row.model.id);
  const prices = useRequest("prices", { models: ids }).data;
  const rows = (prices?.models ?? []).flatMap((row) => tiers(row));
  const copilot = useAgents().includes("copilot-cli");
  return (
    <Panel
      title="Prices used"
      aside="· US dollars per million tokens"
      actions={
        prices ? (
          <span className="subtitle">
            Price table of {fullDate(prices.checked)}, from Anthropic's pricing page
          </span>
        ) : undefined
      }
    >
      <TableScroll label="Prices used">
        <table className="data wide">
          <thead>
            <tr>
              <th scope="col">Model</th>
              <th scope="col">Input</th>
              <th scope="col">Output</th>
              <th scope="col">Cache write, 5 min</th>
              <th scope="col">Cache write, 1 h</th>
              <th scope="col">Cache read</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.name}>
                <th scope="row">{row.name}</th>
                <td>{price(row.input)}</td>
                <td>{price(row.output)}</td>
                <td>{price(row.cacheWrite5m)}</td>
                <td>{price(row.cacheWrite1h)}</td>
                <td>{price(row.cacheRead)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </TableScroll>
      <p className="footnote">
        Fast mode, US-only inference (1.1 times) and web search are priced from the same table. A
        model missing from it shows as unpriced, never as free
        {models.some((row) => unpriced(row.model)) && prices && !prices.refresh.enabled
          ? "; Settings can check this project's repository for a newer table once a day."
          : "."}
        {copilot &&
          " Copilot CLI's requests cost what Copilot CLI recorded, at $0.01 an AI credit, so they need no price."}
      </p>
    </Panel>
  );
}

/** A model's prices, and its prices for long prompts where it has them. */
function tiers(row: PriceRow): {
  name: string;
  input: number;
  output: number;
  cacheWrite5m: number;
  cacheWrite1h: number;
  cacheRead: number;
}[] {
  if (!row.longPrompt) return [{ ...row, name: row.model.name }];
  return [
    { ...row, name: `${row.model.name}, prompts up to ${compact(row.longPrompt.above)}` },
    { ...row.longPrompt, name: `${row.model.name}, longer prompts` },
  ];
}
