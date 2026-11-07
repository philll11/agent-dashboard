// The moment ages and reset times are worked out from: now, or the moment AGENT_DASHBOARD_NOW
// fixes for tests, as it does for agent-dashboard itself.

export function now(): number {
  const fixed = process.env.AGENT_DASHBOARD_NOW;
  const ms = fixed ? Date.parse(fixed) : Number.NaN;
  return Number.isNaN(ms) ? Date.now() : ms;
}
