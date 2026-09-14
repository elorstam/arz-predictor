import type { DailyRun } from "../types";

type Exclusion = NonNullable<DailyRun["exclusions"]>[number];

export function relevantCategoryExclusion(e: Exclusion, category: string): boolean {
  if (category === "TÜMÜ") return true;
  if (e.category !== category) return false;
  // Coverage/history failures occur before a prediction/outcome exists.
  if (e.prediction_id == null) return true;
  if (category === "OVER_25" || category === "OVER_35") {
    return e.market === "TOTAL_GOALS" && e.selection === "OVER"
      && e.line === (category === "OVER_25" ? 2.5 : 3.5);
  }
  if (category === "BTTS_YES") return e.market === "BTTS" && e.selection === "YES";
  return true;
}
