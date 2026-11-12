// Which colour each model gets. Colours follow the model family, in the fixed order the design
// validated (ADR 0007); a family's newest version gets its colour, and every other model shares
// the neutral colour of "other models". Copilot names Claude's models with dots, as in
// `claude-opus-5.5`, so the same version under every agent's id shares the family's colour.

export const SERIES = ["fable", "opus", "sonnet", "haiku", "gpt", "gemini"] as const;
export type Family = (typeof SERIES)[number];
export type Series = Family | "other";

interface Parsed {
  family: Family;
  version: number[];
}

const FAMILIES: [Family, RegExp][] = [
  ["fable", /claude-fable-(\d+)(?:[-.](\d{1,2}))?(?!\d)/],
  ["opus", /claude-opus-(\d+)(?:[-.](\d{1,2}))?(?!\d)/],
  ["sonnet", /claude-sonnet-(\d+)(?:[-.](\d{1,2}))?(?!\d)/],
  ["haiku", /claude-haiku-(\d+)(?:[-.](\d{1,2}))?(?!\d)/],
  ["gpt", /^gpt-(\d+)(?:\.(\d{1,2}))?/],
  ["gemini", /^gemini-(\d+)(?:\.(\d{1,2}))?/],
];

function parse(model: string): Parsed | undefined {
  for (const [family, pattern] of FAMILIES) {
    const match = pattern.exec(model);
    if (match) return { family, version: [Number(match[1]), Number(match[2] ?? 0)] };
  }
  return undefined;
}

function same(a: number[], b: number[]): boolean {
  return !newer(a, b) && !newer(b, a);
}

function newer(a: number[], b: number[]): boolean {
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    const difference = (a[index] ?? 0) - (b[index] ?? 0);
    if (difference !== 0) return difference > 0;
  }
  return false;
}

/** The series of each of `models`: each family's newest version gets the family's colour. */
export function seriesOf(models: Iterable<string>): Map<string, Series> {
  const newest = new Map<Family, number[]>();
  const all = [...models];
  for (const model of all) {
    const parsed = parse(model);
    if (!parsed) continue;
    const best = newest.get(parsed.family);
    if (!best || newer(parsed.version, best)) newest.set(parsed.family, parsed.version);
  }
  const series = new Map<string, Series>();
  for (const model of all) {
    const parsed = parse(model);
    const best = parsed && newest.get(parsed.family);
    series.set(model, parsed && best && same(parsed.version, best) ? parsed.family : "other");
  }
  return series;
}

/** The order series are stacked and listed in. */
export function seriesRank(series: Series): number {
  const index = SERIES.indexOf(series as Family);
  return index < 0 ? SERIES.length : index;
}

/** The CSS colour, or high contrast pattern, of a series. */
export function seriesFill(series: Series): string {
  return `var(--fill-${series})`;
}
