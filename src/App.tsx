import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useBusinessDate } from "./hooks/useBusinessDate";
import { businessClock, type BusinessClockSnapshot } from "./lib/businessClock";
import { useEffect, useState } from "react";
import "./App.css";
import { TodayPage } from "./pages/TodayPage";
import { CandidatesPage } from "./pages/CandidatesPage";
import { CouponsPage } from "./pages/CouponsPage";
import { PopularsPage } from "./pages/PopularsPage";
import { MatchesPage } from "./pages/MatchesPage";
import { ModelPerformancePage } from "./pages/ModelPerformancePage";
import { CouponPerformancePage } from "./pages/CouponPerformancePage";
import { DataCenterPage } from "./pages/DataCenterPage";
import { SettingsPage } from "./pages/SettingsPage";

import { useAsync } from "./hooks/useAsync";
import { api } from "./services/tauri";
import { BrandLogo } from "./components/BrandLogo";
import { ActivationPage } from "./pages/ActivationPage";
import { LicenseBadge } from "./components/LicenseBadge";
import { readinessLabel } from "./lib/format";
import type { LicenseStatus } from "./types";

export type TabId="today"|"candidates"|"coupons"|"populars"|"matches"|"model"|"performance"|"data"|"settings";
const tabs:[TabId,string,string][]=[["today","Bugün","⌂"],["candidates","Günün Adayları","◇"],["coupons","Günün Kuponları","▱"],["populars","Model Destekli Popülerler","☆"],["matches","Maçlar","▣"],["model","Model Performansı","≋"],["performance","Kupon Performansı","↗"],["data","Veri Merkezi","◫"],["settings","Ayarlar","⚙"]];
const pages:Record<Exclude<TabId,"settings">,React.ComponentType>={today:TodayPage,candidates:CandidatesPage,coupons:CouponsPage,populars:PopularsPage,matches:MatchesPage,model:ModelPerformancePage,performance:CouponPerformancePage,data:DataCenterPage};
export default function App(){
 useEffect(()=>{const check=()=>{if(document.visibilityState==="visible")void invoke("automatic_refresh_request",{staleOnly:true}).catch(()=>{});};window.addEventListener("focus",check);document.addEventListener("visibilitychange",check);return()=>{window.removeEventListener("focus",check);document.removeEventListener("visibilitychange",check)}},[]);
 useEffect(()=>{const changed=()=>{api.invalidateDailyViews();window.dispatchEvent(new Event("arz-publication-changed"));window.dispatchEvent(new Event("arz-readiness-changed"))};const unlisten=Promise.all([listen("daily-publication-updated",changed),listen("coupon-settlement-updated",changed)]);return()=>{void unlisten.then(fns=>fns.forEach(fn=>fn()))}},[]);
 const date=useBusinessDate(); const [publicationEpoch,setPublicationEpoch]=useState(0);
 useEffect(()=>{let active=true,busy=false;const check=()=>{businessClock.check();if(busy)return;busy=true;void invoke<BusinessClockSnapshot>("business_clock_get").then(s=>{if(active)businessClock.accept(s)}).catch(()=>{}).finally(()=>{busy=false})};check();const timer=window.setInterval(check,1000);window.addEventListener("focus",check);document.addEventListener("visibilitychange",check);return()=>{active=false;clearInterval(timer);window.removeEventListener("focus",check);document.removeEventListener("visibilitychange",check)}},[]);
 useEffect(()=>{api.invalidateDailyViews();let active=true;void api.ensureDaily(date).then(()=>{if(active)setPublicationEpoch(v=>v+1)},()=>{});return()=>{active=false}},[date]);
 useEffect(()=>{const changed=()=>setPublicationEpoch(v=>v+1);window.addEventListener("arz-publication-changed",changed);return()=>window.removeEventListener("arz-publication-changed",changed)},[]);
 const [tab,setTab]=useState<TabId>(()=>(localStorage.getItem("arz.defaultTab") as TabId)||"today"),[compact,setCompact]=useState(()=>localStorage.getItem("arz.compact")!=="false"),[theme,setTheme]=useState(()=>localStorage.getItem("arz.theme")||"system");
 const readiness=useAsync(api.currentReadiness,[]),licenseState=useAsync(api.licenseStatus,[]); const [licenseData,setLicenseData]=useState<LicenseStatus|null>(null); const Page=pages[tab as Exclude<TabId,"settings">];
 useEffect(()=>{const refresh=()=>readiness.reload();window.addEventListener("arz-readiness-changed",refresh);return()=>window.removeEventListener("arz-readiness-changed",refresh)},[readiness.reload]); useEffect(()=>{if(licenseState.data)setLicenseData(licenseState.data)},[licenseState.data]); useEffect(()=>{const timer=window.setInterval(licenseState.reload,5*60*1000);return()=>window.clearInterval(timer)},[licenseState.reload]); useEffect(()=>{document.documentElement.dataset.theme=theme;localStorage.setItem("arz.theme",theme)},[theme]); useEffect(()=>{document.documentElement.dataset.density=compact?"compact":"comfortable";localStorage.setItem("arz.compact",String(compact))},[compact]);
 const currentLicense=licenseData??{state:"VERIFYING",plan:null,license_id:null,activated_at:null,expires_at:null,last_verified_at:null,device_bound:false,offline_grace_until:null,message:licenseState.error?"Lisans durumu doğrulanamadı.":"Lisans durumu doğrulanıyor."}; const gated=(Boolean(licenseState.error)||(!licenseData&&licenseState.loading)||!['ACTIVE','OFFLINE_GRACE'].includes(currentLicense.state))&&tab!=="settings";
 return <div className="app-shell"><aside className="sidebar"><div className="brand"><BrandLogo/></div><nav aria-label="Ana navigasyon">{tabs.map(([id,label,icon])=><button key={id} className={tab===id?"active":""} onClick={()=>setTab(id)}><span className="nav-icon" aria-hidden="true">{icon}</span><span className="nav-label">{label}</span></button>)}</nav><div className="sidebar-foot"><span className={`health-dot ${readiness.data?.can_generate_predictions.ready?"online":"offline"}`}/><div><strong>{readiness.data?readinessLabel(readiness.data.overall_status):"Hazırlık denetleniyor"}</strong><small>{date}</small></div></div></aside><main className="workspace"><div className="topbar"><span>Yerel ve gizli model analitiği</span><div><LicenseBadge license={currentLicense}/><span className="business-chip">İstanbul · {date}</span><button className="icon-button" title="Görünüm yoğunluğu" aria-label="Görünüm yoğunluğunu değiştir" onClick={()=>setCompact(v=>!v)}>↕</button></div></div><div className="page">{tab==="settings"?<SettingsPage license={currentLicense} onLicense={v=>setLicenseData(v)}/>:gated?<ActivationPage status={currentLicense} onActivated={v=>setLicenseData(v)}/>:tab==="data"?<><div className="license-diagnostic"><span>Lisans durumu</span><strong>{currentLicense.state}</strong><small>{currentLicense.expires_at?`Bitiş: ${new Date(currentLicense.expires_at).toLocaleDateString("tr-TR")}`:currentLicense.message}</small></div><Page key={["today","candidates","coupons","populars","data"].includes(tab)?`${date}:${publicationEpoch}`:tab}/></>:<Page key={["today","candidates","coupons","populars","data"].includes(tab)?`${date}:${publicationEpoch}`:tab}/>}</div></main><SettingsBridge setTheme={setTheme} setCompact={setCompact}/></div>
}
function SettingsBridge({setTheme,setCompact}:{setTheme:(v:string)=>void;setCompact:(v:boolean)=>void}){useEffect(()=>{const fn=(e:Event)=>{const d=(e as CustomEvent).detail;if(d?.theme)setTheme(d.theme);if(typeof d?.compact==="boolean")setCompact(d.compact)};window.addEventListener("arz-settings",fn);return()=>window.removeEventListener("arz-settings",fn)},[setTheme,setCompact]);return null}
