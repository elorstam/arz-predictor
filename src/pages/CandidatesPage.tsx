import { useState } from "react";
import { useAsync } from "../hooks/useAsync";
import { api } from "../services/tauri";
import { businessDate, categoryLabel, percent } from "../lib/format";
import { DataState, EmptyState } from "../components/States";
import { MatchRow } from "../components/MatchRow";
import { PageHeader } from "../components/Page";
import { StatusBadge } from "../components/StatusBadge";
const categories=["TÜMÜ","CORNERS","OVER_25","OVER_35","BTTS_YES","HIGH_CONFIDENCE","SURPRISE","COMPOUND"];
export function CandidatesPage(){
 const date=businessDate(),[category,setCategory]=useState("TÜMÜ");
 const state=useAsync(async()=>{const [run,matches]=await Promise.all([api.candidates(date).catch(()=>null),api.matches()]);return{run,matches}},[date]);
 return <><PageHeader eyebrow="GÜNLÜK MODEL SEÇİMLERİ" title="Günün Adayları" description="Kalibre model olasılığı, güncel oran ve mevcut yeterlilik politikası." actions={<button className="button secondary" onClick={state.reload}>Yenile</button>}/><div className="filterbar category-filters" role="group" aria-label="Kategori filtresi">{categories.map(x=><button key={x} className={category===x?"active":""} onClick={()=>setCategory(x)}>{x==="TÜMÜ"?"Tümü":categoryLabel(x)}</button>)}</div><DataState {...state} onRetry={state.reload}>{({run,matches})=>{if(!run)return <section className="panel"><EmptyState title="Bugün yeterli güvene sahip aday yok." detail="Yeni aday çalıştırması oluştuğunda burada görünecek."/></section>;const items=run.candidates.filter(x=>category==="TÜMÜ"||x.category===category);return <section className="panel"><div className="context-strip"><div><StatusBadge value={run.prediction_context??"BASE"}/>{run.prediction_context==="LINEUP_AWARE"&&<span>11’ler sonrası güncellendi</span>}</div><span>{items.length} aday · Çalıştırma #{run.run_id}</span></div>{items.length?<div className="candidate-grid">{items.map(c=><div key={c.id} className="candidate-wrap"><MatchRow candidate={c} match={matches.find(m=>m.match_id===c.match_id)}/>{c.base_public_probability!=null&&c.final_public_probability!=null&&c.base_public_probability!==c.final_public_probability&&<div className="lineup-delta">BASE {percent(c.base_public_probability,1)} <span>→</span> Güncel {percent(c.final_public_probability,1)}</div>}{c.prediction_source==="BASE_RETAINED"&&<small className="retained-note">11 bilgisi mevcut, model fayda görmedi; BASE korundu.</small>}</div>)}</div>:<EmptyState title="Bugün bu kategori için yeterli güvene sahip aday yok."/>}</section>}}</DataState></>
}
