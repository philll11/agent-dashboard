// Plan windows as every surface names, orders and labels them: the dashboard, and VS Code's
// status bar and sidebar.

import type { Level, WindowReading } from "@agent-dashboard/protocol";

import { PLANS } from "./agents";
import { grouped } from "./format";

/** What a window is called: `Five-hour window`, `Weekly, all models`, `Weekly, Fable`, or Copilot's
 * `Copilot allowance`. */
export function windowName(reading: WindowReading): string {
  if (reading.window === "five_hour") return "Five-hour window";
  if (reading.window === "seven_day") return "Weekly, all models";
  if (reading.window === "monthly") return "Copilot allowance";
  if (reading.model) return `Weekly, ${reading.model}`;
  if (reading.window === "spend_limit") return "Spend limit";
  return reading.window;
}

/** What a screen reader calls a window's meter: `Weekly Fable window`. */
export function meterName(reading: WindowReading): string {
  if (reading.window === "five_hour") return "Five-hour window";
  if (reading.window === "monthly") return "Copilot's monthly allowance";
  if (reading.window === "seven_day") return "Weekly window";
  if (reading.model) return `Weekly ${reading.model} window`;
  return windowName(reading);
}

function rank(reading: WindowReading): number {
  if (reading.window === "five_hour") return 0;
  if (reading.window === "seven_day") return 1;
  if (reading.window === "monthly") return 4;
  if (reading.model) return 2;
  return 3;
}

/** Claude Code's windows first: five-hour, then weekly, then each model's weekly window, then any
 * other; then Copilot's allowance. */
export function inOrder(readings: readonly WindowReading[]): WindowReading[] {
  return [...readings].sort(
    (a, b) => rank(a) - rank(b) || (a.model ?? a.window).localeCompare(b.model ?? b.window),
  );
}

/** A window's name under its plan's heading, where Copilot's allowance is its `Monthly allowance`. */
export function groupedName(reading: WindowReading): string {
  return reading.window === "monthly" ? "Monthly allowance" : windowName(reading);
}

/** The readings of each plan that has any, Claude's first, each in order. */
export function byPlan(
  readings: readonly WindowReading[],
): { plan: string; readings: WindowReading[] }[] {
  const ordered = inOrder(readings);
  const plans = [
    ...PLANS,
    ...ordered.map((reading) => reading.plan).filter((plan) => !PLANS.some((p) => p === plan)),
  ];
  return [...new Set(plans)]
    .map((plan) => ({ plan, readings: ordered.filter((reading) => reading.plan === plan) }))
    .filter((group) => group.readings.length > 0);
}

/** How much of an allowance is used, where the agent says: `1,575 of 45,000 AI credits`. */
export function creditsText(reading: WindowReading): string | undefined {
  if (reading.used == null || reading.entitlement == null) return undefined;
  return `${grouped(reading.used)} of ${grouped(reading.entitlement)} AI credits`;
}

export const LEVEL_TEXT: Record<Level, string | undefined> = {
  normal: undefined,
  warning: "Over 70%",
  critical: "Over 90%",
};
