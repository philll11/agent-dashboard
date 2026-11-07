// The marks plan windows carry: a window's level, and where its reading comes from.

import type { Level, WindowReading } from "@agent-dashboard/protocol";

import { ago } from "../format";
import { CheckCircle, Clock, Warning } from "../ui/icons";

const SOURCES: Record<WindowReading["provenance"], string> = {
  official: "Official · status line",
  recorded: "Recorded · Copilot CLI",
  experimental: "Experimental · usage endpoint",
};

/** A warning mark in the colour of its level. */
export function LevelIcon({ level, size = 13 }: { level: Level; size?: number }) {
  return (
    <span className={`level-icon ${level}`}>
      <Warning size={size} />
    </span>
  );
}

/** Where the newest reading comes from, and how old it is: `Official · status line · 2 min ago`,
 * `Recorded · Copilot CLI · 1 h ago`. */
export function Provenance({
  readings,
  nowMs,
}: {
  readings: readonly WindowReading[];
  nowMs: number;
}) {
  const newest = readings.reduce<WindowReading | undefined>(
    (best, reading) => (best && best.observedMs >= reading.observedMs ? best : reading),
    undefined,
  );
  if (!newest) return null;
  const source = SOURCES[newest.provenance];
  if (newest.stale) {
    return (
      <span className="badge caution">
        <Clock size={12} />
        {source} · {ago(newest.observedMs, nowMs)}
      </span>
    );
  }
  return (
    <span className="badge">
      <CheckCircle size={12} />
      {source} · {ago(newest.observedMs, nowMs)}
    </span>
  );
}
