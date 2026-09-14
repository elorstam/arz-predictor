import { useEffect, useState } from "react";
import { publishedDailyCoupons, couponTypeOrder } from "../lib/readyCoupons";
import { katlamaStatus } from "../lib/katlamaStatus";
import { useBusinessDate } from "../hooks/useBusinessDate";
import { useAsync } from "../hooks/useAsync";
import { api } from "../services/tauri";
import { decimal, marketLabel, money, percent } from "../lib/format";
import type { CompoundSeries, CompoundSearch, Coupon, UpcomingMatch } from "../types";
import { DataState, EmptyState } from "../components/States";
import { PageHeader } from "../components/Page";
import { StatusBadge } from "../components/StatusBadge";
import { BttsCouponDiagnostics } from "../components/BttsCouponCard";
import "./CouponsPage.css";
const names: Record<string, string> = { DAILY_CORNERS: "Korner", DAILY_OVER_25: "2.5 Üst", DAILY_OVER_35: "3.5 Üst", DAILY_BTTS: "KG Var", DAILY_HIGH_CONFIDENCE: "Yüksek Güven", DAILY_SURPRISE_SYSTEM: "Sürpriz Sistem", DAILY_COMPOUND: "Katlama" };
export function CouponsPage() {
  const date = useBusinessDate();
  const state = useAsync(async () => {
    const [output, matches, compound, search] = await Promise.all([api.dailyOutput(date), api.dailyMatches(date), api.compound(), api.compoundSearch(date)]);
    return { ...output, coupons: output.coupons.filter(c => c.business_date === date), matches, compound, search };
  }, [date]);
  useEffect(() => { if (state.data) void api.prioritizeMatches([...new Set(state.data.coupons.flatMap(c => c.selections.map(s => s.match_id)))]).catch(() => {}); }, [state.data]);
  useEffect(() => { window.addEventListener("btts-updated", state.reload); return () => window.removeEventListener("btts-updated", state.reload); }, [state.reload]);
  return <div className="daily-coupons-page"><PageHeader eyebrow="GÜNÜN SEÇİMLERİ" title="Günün Kuponları" description="Modelin seçtiği maçlar. Tek bakışta olasılık, oran ve kupon." actions={<button className="button secondary" onClick={state.reload}>Yenile</button>} />
    <DataState {...state} onRetry={state.reload}>{({ coupons, matches, compound, run, search }) => {
      const ready = publishedDailyCoupons(coupons, date);
      return <><CompoundStatus date={date} series={compound} search={search} ready={ready.some(c => c.coupon_type === "DAILY_COMPOUND" && c.publication_status === "READY")} onChange={state.reload} />
        <div className="daily-coupon-heading"><h2>Yayımlanan kuponlar <span>{ready.length}</span></h2><span>{date} · İstanbul</span></div>
        {ready.length ? <div className="coupon-grid">{ready.map(coupon => <CouponCard key={coupon.id} coupon={coupon} matches={matches} />)}</div> : <EmptyState title="Henüz yayımlanmış kupon yok" detail="Uygun seçimler bulunduğunda kuponlar otomatik olarak burada görünür." />}
        <details className="daily-coupon-technical"><summary>Diğer kategoriler ve teknik ayrıntılar</summary><div className="daily-unavailable">{couponTypeOrder.filter(kind => !ready.some(c => c.coupon_type === kind)).map(kind => <div key={kind}><strong>{names[kind]}</strong><span>{kind === "DAILY_COMPOUND" ? katlamaStatus(compound, date, false, search) : "Bugün yeterli uygun seçim bulunamadı"}</span></div>)}</div>
          {!ready.some(c => c.coupon_type === "DAILY_BTTS") && <BttsCouponDiagnostics date={date} />}
          <p>Yayın {run.publication_id} · {run.prediction_context}</p><details><summary>Katlama kombinasyon araması</summary><p>{search.pool_size} uygun aday · {search.pairs_evaluated} ikili · {search.triples_evaluated} üçlü · {search.valid_combinations} uygun kombinasyon</p><p>2–3 seçim · toplam oran 1,50–1,80 · hedef 1,80</p></details><details><summary>Aday eleme nedenleri</summary>{run.exclusions?.map((e, i) => <p key={i}>{e.category} · {e.reason}</p>)}</details>
        </details></>;
    }}</DataState></div>;
}
function CouponCard({ coupon, matches }: { coupon: Coupon; matches: UpcomingMatch[] }) {
  const check = useAsync(() => api.couponImpact(coupon.id), [coupon.id], false);
  return <article className="panel coupon-card" data-coupon-type={coupon.coupon_type} data-coupon-id={coupon.id}>
    <header className="daily-coupon-header"><div><span className="eyebrow">{coupon.selections.length} SEÇİM{coupon.step_number ? ` · ADIM ${coupon.step_number}` : ""}</span><h2>{names[coupon.coupon_type]}</h2></div><StatusBadge value={coupon.settlement_result ?? coupon.publication_status} /></header>
    <div className="daily-coupon-odd"><span>{coupon.system_sizes.length ? "Sistem" : "Toplam oran"}</span><strong>{coupon.system_sizes.length ? coupon.system_sizes.join(" / ") : decimal(coupon.combined_decimal_odd)}</strong>{coupon.system_sizes.length > 0 && <small>{coupon.columns.length} kolon</small>}</div>
    <div className="daily-selection-labels"><span>Maç / seçim</span><span>Olasılık</span><span>Oran</span></div><div className="daily-selection-list">{coupon.selections.map(s => {
      const match = matches.find(m => m.match_id === s.match_id);
      return <div className="daily-selection" key={s.candidate_id}><div><small>{match?.competition ?? "Maç"}</small><strong>{match ? `${match.home_team} — ${match.away_team}` : `Maç #${s.match_id}`}</strong><span>{marketLabel(s.market, s.selection, s.line)}</span></div><strong className="daily-probability">{percent(s.public_probability, 1)}</strong><strong className="daily-odd">{decimal(s.odd)}</strong></div>;
    })}</div>
    <footer className="daily-coupon-footer"><span>{coupon.total_stake_cents != null ? <>Tutar <strong>{money(coupon.total_stake_cents)}</strong></> : "Model seçimi"}</span><details className="daily-card-actions"><summary aria-label={`${names[coupon.coupon_type]} teknik işlemler`}>Ayrıntılar</summary>
      <button className="button secondary" onClick={check.reload} disabled={check.loading}>11 etkisini kontrol et</button>{check.error && <p role="alert">{check.error}</p>}
      {check.data && <p>{check.data.revision_available ? "11 sonrası değişiklikler mevcut; kayıtlı seçimler korunuyor." : "Yeni 11 değişikliği yok."}</p>}{check.data?.items.filter(i => i.impact_type !== "UNCHANGED").map((i, n) => <p key={n}>{marketLabel(i.market, i.selection, i.line)} · {percent(i.base_probability, 1)} → {percent(i.latest_probability, 1)}</p>)}
      {coupon.columns.length > 0 && <details><summary>Kolon ayrıntıları</summary>{coupon.columns.map(c => <p key={c.column_number}>#{c.column_number} · Sistem {c.system_size} · {decimal(c.column_decimal_odd)} <StatusBadge value={c.status} /></p>)}</details>}
      {coupon.coupon_type === "DAILY_BTTS" && <BttsCouponDiagnostics date={coupon.business_date} />}
    </details></footer></article>;
}
function CompoundStatus({ date, series, ready, search, onChange }: { date: string; series: CompoundSeries | null; ready: boolean; search: CompoundSearch | null; onChange: () => void }) {
  const [stake, setStake] = useState("100"), [busy, setBusy] = useState(false), [error, setError] = useState("");
  const pending = series?.history.some(s => s.result === "UNSETTLED") ?? false;
  async function act() { setBusy(true); setError(""); try { if (!series || series.status !== "ACTIVE") await api.startCompound(date, Math.round(Number(stake) * 100)); else await api.generateCompound(date); onChange(); } catch (e) { setError(e instanceof Error && e.message === "NO_QUALIFYING_COMPOUND_COUPON" ? "Bugün uygun Katlama kombinasyonu bulunamadı" : String(e)); } finally { setBusy(false); } }
  return <section className="daily-katlama" aria-label="Katlama durumu"><div className="daily-katlama-icon" aria-hidden="true">↗</div><div className="daily-katlama-title"><h2>Katlama <span>Adım {series?.current_step ?? 1} / 7</span></h2><p data-katlama-state>{katlamaStatus(series, date, ready, search)}</p></div><div className="daily-katlama-steps" aria-hidden="true">{Array.from({ length: 7 }, (_, i) => <i key={i} className={i < (series?.current_step ?? 1) ? "active" : ""} />)}</div>
    {series?.status === "ACTIVE" && <div className="daily-katlama-stake"><small>Devreden tutar</small><strong>{money(series.current_stake_cents)}</strong></div>}<details className="daily-katlama-actions"><summary>{series?.status === "ACTIVE" ? "Seri ayrıntıları" : "Seriyi başlat"}</summary><div>
      {(!series || series.status !== "ACTIVE") && <label>Başlangıç tutarı <input aria-label="Katlama başlangıç tutarı" type="number" min="1" value={stake} onChange={e => setStake(e.target.value)} /></label>}<p>2–3 seçim · hedef oran 1,80. Kesinleşen sonuç sonrası adım otomatik hazırlanır.</p>
      <button className="button secondary" disabled={busy || pending || !Number.isFinite(Number(stake)) || Number(stake) <= 0} onClick={act}>{busy ? "İşleniyor…" : series?.status === "ACTIVE" ? "Adım kuponunu kontrol et" : "Seriyi başlat"}</button>{series && <p>Başlangıç {money(series.starting_stake_cents)} · {series.history.length} kayıtlı adım</p>}{series?.history.map(s => <p key={s.coupon_id}>{s.business_date} · Adım {s.step_number} · {s.result === "UNSETTLED" ? "Sonuç bekleniyor" : s.result}</p>)}{error && <p role="alert">{error}</p>}
    </div></details></section>;
}
