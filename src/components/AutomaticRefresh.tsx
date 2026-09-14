import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
type Status={settings:{enabled:boolean;interval_seconds:number};state:string;last_success:number|null;next_check:number|null;error:string|null};
export function AutomaticRefresh({settings=false}:{settings?:boolean}){
 const [state,setState]=useState<Status|null>(null),[error,setError]=useState("");
 useEffect(()=>{let active=true;const refresh=()=>invoke<Status>("automatic_refresh_status").then(s=>{if(active)setState(s)},()=>{});void refresh();const timer=setInterval(refresh,10000);return()=>{active=false;clearInterval(timer)}},[]);
 const time=(t:number|null)=>t?new Date(t*1000).toLocaleTimeString("tr-TR"):"—";
 async function update(enabled:boolean,interval_seconds:number){try{setState(await invoke<Status>("automatic_refresh_configure",{settings:{enabled,interval_seconds}}));setError("")}catch(e){setError(String(e))}}
 return <div className="panel automatic-refresh"><strong>Otomatik yenileme: {state?state.settings.enabled?"AKTİF":"KAPALI":"Kontrol ediliyor"}</strong><span>{state?.state!=="IDLE"&&state?"Yenileniyor…":""}</span><span>Son başarılı yenileme: {time(state?.last_success??null)}</span><span>Sonraki kontrol: {state?.settings.enabled?time(state.next_check):"—"}</span>{state?.error&&<details><summary>Yenileme hatası</summary>{state.error}</details>}{settings&&state&&<><label><input type="checkbox" checked={state.settings.enabled} onChange={e=>void update(e.target.checked,state.settings.interval_seconds)}/> Otomatik veri yenileme</label><label>Aralık <select value={state.settings.interval_seconds} onChange={e=>void update(state.settings.enabled,Number(e.target.value))}><option value={300}>5 dakika</option><option value={600}>10 dakika</option><option value={900}>15 dakika</option></select></label></>}{error&&<span role="alert">{error}</span>}</div>
}
