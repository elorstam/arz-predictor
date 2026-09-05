import type { Candidate, UpcomingMatch } from "../types";
import { decimal, localDate, marketLabel, percent } from "../lib/format";
import { useAsync } from "../hooks/useAsync";
import { api, cachedLogo } from "../services/tauri";
import { StatusBadge } from "./StatusBadge";
import { TeamLogo } from "./TeamLogo";

export function MatchRow({match,candidate,onClick}:{match?:UpcomingMatch;candidate?:Candidate;onClick?:()=>void}){
 const homeAsset=useAsync(()=>match?cachedLogo("TEAM",match.home_team_id):Promise.resolve(null),[match?.home_team_id]);
 const awayAsset=useAsync(()=>match?cachedLogo("TEAM",match.away_team_id):Promise.resolve(null),[match?.away_team_id]);
 const home=match?.home_team??`Maç #${candidate?.match_id??"—"}`,away=match?.away_team??"Takım bilgisi bekleniyor";
 const homeLogo=homeAsset.data?api.fileUrl(homeAsset.data):null,awayLogo=awayAsset.data?api.fileUrl(awayAsset.data):null;
 const body=<><div className="match-meta"><span>{match?.competition??`Lig #${candidate?.competition_id??"—"}`}</span><time>{localDate(match?.kickoff_at)}</time></div><div className="teams"><span><TeamLogo name={home} src={homeLogo}/>{home}</span><b>—</b><span>{away}<TeamLogo name={away} src={awayLogo}/></span></div>{candidate&&<div className="prediction"><strong>{marketLabel(candidate.market,candidate.selection,candidate.line)}</strong><span className="probability">{percent(candidate.public_probability,1)}</span><span>{decimal(candidate.iddaa_odd)} oran</span><span className="muted">Edge {percent(candidate.probability_edge,1)} · EV {percent(candidate.expected_value,1)}</span><StatusBadge value={candidate.prediction_source}/></div>}</>;
 return onClick?<button className="match-row interactive" onClick={onClick}>{body}</button>:<div className="match-row">{body}</div>
}
