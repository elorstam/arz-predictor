import { selectionStatusLabel } from "../lib/dailySelections";
import type { Candidate, UpcomingMatch } from "../types";
import { useEffect, useRef, useState } from "react";
import { categoryLabel, decimal, localDate, marketLabel, percent } from "../lib/format";
import { useAsync } from "../hooks/useAsync";
import { api, cachedLogo } from "../services/tauri";
import { StatusBadge } from "./StatusBadge";
import { TeamLogo } from "./TeamLogo";

export function MatchRow({match,candidate,onClick}:{match?:UpcomingMatch;candidate?:Candidate;onClick?:()=>void}){
 const rowRef=useRef<HTMLDivElement & HTMLButtonElement>(null),[visible,setVisible]=useState(false);
 useEffect(()=>{const node=rowRef.current;if(!node)return;const observer=new IntersectionObserver(entries=>setVisible(entries.some(x=>x.isIntersecting)),{rootMargin:"100px"});observer.observe(node);return()=>observer.disconnect()},[]);
 const homeAsset=useAsync(()=>match&&visible?cachedLogo("TEAM",match.home_team_id):Promise.resolve(null),[match?.home_team_id,visible]);
 const awayAsset=useAsync(()=>match&&visible?cachedLogo("TEAM",match.away_team_id):Promise.resolve(null),[match?.away_team_id,visible]);
 useEffect(()=>{if(!match||!visible)return;const timer=setInterval(()=>{if(!homeAsset.data)homeAsset.reload();if(!awayAsset.data)awayAsset.reload();},16000);return()=>clearInterval(timer)},[visible,match?.home_team_id,match?.away_team_id,homeAsset.data,awayAsset.data,homeAsset.reload,awayAsset.reload]);
 const home=match?.home_team??`Maç #${candidate?.match_id??"—"}`,away=match?.away_team??"Takım bilgisi bekleniyor";
 const homeLogo=homeAsset.data?api.fileUrl(homeAsset.data):null,awayLogo=awayAsset.data?api.fileUrl(awayAsset.data):null;
 const body=<><StatusBadge value={match?.model_coverage}/><div className="match-meta"><span>{match?.competition??`Lig #${candidate?.competition_id??"—"}`}</span><time>{localDate(match?.kickoff_at)}</time></div><div className="teams"><span><TeamLogo name={home} src={homeLogo}/>{home}</span><b>—</b><span>{away}<TeamLogo name={away} src={awayLogo}/></span></div>{candidate&&<div className="prediction"><strong>{marketLabel(candidate.market,candidate.selection,candidate.line)}</strong><span className="probability">{percent(candidate.public_probability,1)}</span><span>{decimal(candidate.iddaa_odd)} oran</span><span className="muted">Edge {percent(candidate.probability_edge,1)} · EV {percent(candidate.expected_value,1)}</span><span className="muted">{categoryLabel(candidate.category)}</span><StatusBadge value={candidate.prediction_source}/><span className="badge" data-selection-status={candidate.selection_status}>{selectionStatusLabel(candidate.selection_status)}</span><span className="muted">Güven {candidate.confidence!=null?percent(candidate.confidence,1):"—"} · Veri {candidate.data_quality??"—"} · Örnek {candidate.bucket_sample_size??"—"}</span></div>}</>;
 return onClick?<button ref={rowRef} className="match-row interactive" onClick={onClick}>{body}</button>:<div ref={rowRef} className="match-row">{body}</div>
}
