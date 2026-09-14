import { describe, expect, it } from "vitest";
import { relevantCategoryExclusion } from "./goalExclusions";

describe("goal-market exclusion explanations", () => {
  const base = { category: "BTTS_YES", prediction_id: 10, match_id: 1, reason: "ODDS_UNAVAILABLE", market: "TOTAL_GOALS", selection: "OVER", line: 5.5 };
  it("does not present unrelated missing odds as missing BTTS odds", () => {
    expect(relevantCategoryExclusion(base, "BTTS_YES")).toBe(false);
    expect(relevantCategoryExclusion({ ...base, market: "BTTS", selection: "NO", line: null }, "BTTS_YES")).toBe(false);
    expect(relevantCategoryExclusion({ ...base, market: "BTTS", selection: "YES", line: null }, "BTTS_YES")).toBe(true);
  });
  it("keeps exact goal lines and pre-prediction coverage failures", () => {
    const over = { ...base, category: "OVER_25", line: 2.5 };
    expect(relevantCategoryExclusion(over, "OVER_25")).toBe(true);
    expect(relevantCategoryExclusion({ ...over, line: 3.5 }, "OVER_25")).toBe(false);
    expect(relevantCategoryExclusion({ ...over, prediction_id: null, reason: "INSUFFICIENT_HISTORY" }, "OVER_25")).toBe(true);
    expect(relevantCategoryExclusion(over, "BTTS_YES")).toBe(false);
    expect(relevantCategoryExclusion(base, "TÜMÜ")).toBe(true);
  });
});
