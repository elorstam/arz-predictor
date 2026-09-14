import { useState } from "react";
import { useAsync } from "../hooks/useAsync";
import { api } from "../services/tauri";
import { businessDate, localDate } from "../lib/format";

export interface BttsAudit {
 business_date:string; as_of:string; stages:Record<string,number>;
 primary_exclusions:Record<string,number>; coupon_id:number|null;
 matches:{match_id:number;home:string;away:string;prediction_ready:boolean;primary_exclusion:string|null;detail?:string}[];
}
const reasonLabels:Record<string,string>={NO_BTTS_PREDICTION:"KG Var/Yok tahmini yok",NO_BTTS_ODDS:"KG Var oranı yok",INVALID_ODDS:"Geçersiz oran",STALE_ODDS:"24 saatten eski oran",UNRESOLVED:"Eşleşme çözümlenmemiş",INSUFFICIENT_HISTORY:"Yetersiz geçmiş",POOR_MODEL_QUALITY:"Model çıktısı veya kalite sorunu",DUPLICATE:"Tekrarlanan maç",OTHER:"Başlamış/iptal maç veya diğer geçersiz girdi"};
const stages:[string,string][]=[["prediction_ready","Tahmine hazır"],["btts_probability","KG Var olasılığı"],["calibrated_btts_probability","Kalibre olasılık"],["public_btts_fallback","Doğrulanmış model olasılığı"],["btts_odds","BTTS oranı"],["valid_yes_mapping","KG Var eşlemesi"],["invalid_input_survivors","Geçerli girdiler"],["ranking_pool","Sıralama havuzu"],["selected","Seçilen"]];

export function BttsCouponDiagnostics({date}:{date:string}) {
 const state=useAsync(()=>api.bttsLatest(date),[date]);
 const [busy,setBusy]=useState(false),[error,setError]=useState("");
 async function refresh(){setBusy(true);setError("");try{await api.bttsRefresh(date);await state.reload();window.dispatchEvent(new Event("btts-updated"));}catch(e){setError(e instanceof Error?e.message:String(e));}finally{setBusy(false);}}
 const audit=state.data;
 return <div className="btts-diagnostics">
  {date>=businessDate()&&<button className="button secondary" onClick={refresh} disabled={busy}>{busy?"KG Var yenileniyor…":"KG Var oranlarını ve kuponunu yenile"}</button>}
  {(error||state.error)&&<p role="alert">KG Var yenilenemedi: {error||state.error}</p>}
  {state.loading&&!audit&&<p>KG Var akışı denetleniyor…</p>}
  {audit&&<details className="details"><summary>KG Var aşama sayıları ve eleme nedenleri</summary><p>Veri kesiti: {localDate(audit.as_of,true)}</p><p>Olasılık → EV → güven → veri kalitesi. %64 ve pozitif EV eleme şartı uygulanmaz.</p>{stages.map(([key,label])=><div key={key}>{label}: <strong>{audit.stages[key]??0}</strong></div>)}<p>Her tahmine hazır maç için tek ana eleme nedeni:</p>{Object.entries(audit.primary_exclusions).map(([key,n])=><div key={key}>{reasonLabels[key]??key}: {n}</div>)}<details><summary>Maç bazında nedenler</summary>{audit.matches.filter(m=>m.prediction_ready&&m.primary_exclusion).map(m=><p key={m.match_id}>{m.home} — {m.away}: {reasonLabels[m.primary_exclusion!]??m.primary_exclusion}</p>)}</details></details>}
 </div>;
}
