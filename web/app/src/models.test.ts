import { describe, expect, it } from "vitest";

import { seriesOf } from "./models";

describe("model series", () => {
  it("give each family's newest version its colour", () => {
    const series = seriesOf([
      "claude-opus-5-5",
      "claude-opus-5",
      "claude-sonnet-4-5-20250929",
      "claude-opus-4-20250514",
      "glm-4.6",
    ]);
    expect(series.get("claude-opus-5-5")).toBe("opus");
    expect(series.get("claude-opus-5")).toBe("other");
    expect(series.get("claude-opus-4-20250514")).toBe("other");
    expect(series.get("claude-sonnet-4-5-20250929")).toBe("sonnet");
    expect(series.get("glm-4.6")).toBe("other");
  });

  it("share a family's colour between the same version under each agent's id", () => {
    const series = seriesOf([
      "claude-opus-5-5",
      "claude-opus-5.5",
      "claude-opus-5",
      "gpt-6-luna",
      "gpt-5.4-nano",
      "gpt-4o-mini",
      "gemini-3.1-pro",
      "gemini-2.5-flash",
    ]);
    expect(series.get("claude-opus-5-5")).toBe("opus");
    expect(series.get("claude-opus-5.5")).toBe("opus");
    expect(series.get("claude-opus-5")).toBe("other");
    expect(series.get("gpt-6-luna")).toBe("gpt");
    expect(series.get("gpt-5.4-nano")).toBe("other");
    expect(series.get("gpt-4o-mini")).toBe("other");
    expect(series.get("gemini-3.1-pro")).toBe("gemini");
    expect(series.get("gemini-2.5-flash")).toBe("other");
  });
});
