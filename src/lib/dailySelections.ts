import type { Candidate, DailyRun } from "../types";

export function dailySelections(run: DailyRun, category: string): Candidate[] {
  const pool = run.candidates.filter(c => category === "TÜMÜ" || c.category === category);
  const seen = new Set<string>();
  return pool.filter(c => {
    const key = JSON.stringify([c.match_id, c.market, c.selection, c.line]);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export function selectedContext(run: DailyRun): string {
  return run.prediction_context === "LINEUP_AWARE" && run.lineup_verified === true ? "LINEUP_AWARE" : "BASE";
}

export const selectionStatusLabel = (status?: string) => ({
  STRICT_QUALIFIED: "Nitelikli",
  DAILY_RANKED: "Günlük sıralama",
  HIGH_CONFIDENCE: "Yüksek güven · nitelikli",
  KATLAMA_ELIGIBLE: "Katlama uygun",
}[status ?? ""] ?? "");
