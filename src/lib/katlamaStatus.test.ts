import { describe, expect, it } from "vitest";
import { katlamaStatus } from "./katlamaStatus";
import type { CompoundSearch, CompoundSeries } from "../types";
const search = { chosen: null, reason: "NO_VALID_COMBINATION" } as CompoundSearch;
const series = (result: string, step = 2): CompoundSeries => ({ id: 1, business_date: "2026-09-13", status: "ACTIVE", current_step: step, starting_stake_cents: 10000, current_stake_cents: 16900, completed_steps: step - 1, latest_coupon_id: 1, reset_count: 0, history: [{ coupon_id: 1, step_number: 1, business_date: "2026-09-13", result, settled_at: null, stake_cents: 10000, combined_odd: 1.69 }] });
describe("Katlama settlement labels", () => {
  it.each(["WON", "LOST", "VOID"])("does not show false pending after %s without a combination", result => {
    expect(katlamaStatus(series(result), "2026-09-14", false, search)).toBe("Bugün uygun Katlama kombinasyonu bulunamadı");
  });
  it("preserves pending even when an unrelated next combination exists", () => {
    expect(katlamaStatus(series("UNSETTLED", 1), "2026-09-14", false, { ...search, chosen: { candidate_ids: [2, 3], combined_odd: 1.69, quality: 1 } })).toBe("Önceki sonuç henüz kesinleşmedi");
  });
  it("distinguishes a new active coupon from yesterday's unresolved step", () => {
    const value = series("WON"); value.history.push({ ...value.history[0], coupon_id: 2, step_number: 2, business_date: "2026-09-14", result: "UNSETTLED" });
    expect(katlamaStatus(value, "2026-09-14", true, search)).toBe("Aktif seri · seçimler sonuç bekliyor");
  });
  it("reports a loss reset and search failure accurately", () => {
    expect(katlamaStatus(series("LOST", 1), "2026-09-14", false, null)).toContain("seri sıfırlandı");
    expect(katlamaStatus(series("WON"), "2026-09-14", false, { ...search, reason: "SEARCH_LIMIT_REACHED" })).toContain("tamamlanamadı");
  });
});
