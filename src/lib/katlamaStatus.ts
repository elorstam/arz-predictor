import type { CompoundSearch, CompoundSeries } from "../types";
export function katlamaStatus(series: CompoundSeries | null, date: string, ready: boolean, search: CompoundSearch | null): string {
  if (!series || series.status !== "ACTIVE") return "Seri başlatılmadı";
  if (series.manually_reset) return "Seri manuel olarak sıfırlandı";
  const pending = series.history.find(step => step.result === "UNSETTLED");
  if (pending) return pending.business_date !== date ? "Önceki sonuç henüz kesinleşmedi" : ready ? "Aktif seri · seçimler sonuç bekliyor" : "Aktif adımın sonucu bekleniyor";
  if (search && !search.chosen) return search.reason === "SEARCH_LIMIT_REACHED" ? "Katlama kombinasyon araması tamamlanamadı" : "Bugün uygun Katlama kombinasyonu bulunamadı";
  const previous = series.history[series.history.length - 1];
  if (previous?.result === "LOST") return "Önceki seçim kaybetti · seri sıfırlandı";
  if (previous?.result === "WON") return previous.step_number === 7 ? "7 adım tamamlandı · yeni seri hazır" : "Önceki adım kazandı · sonraki adım hazır";
  if (previous?.result === "VOID") return "Önceki adım iptal · aynı adım ve tutar korunuyor";
  return "Aktif seri";
}
