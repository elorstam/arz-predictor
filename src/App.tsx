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
import { businessDate } from "./lib/format";
import { useAsync } from "./hooks/useAsync";
import { api } from "./services/tauri";
import { BrandLogo } from "./components/BrandLogo";
import { ActivationPage } from "./pages/ActivationPage";
import { LicenseBadge } from "./components/LicenseBadge";
import type { LicenseStatus } from "./types";

export type TabId="today"|"candidates"|"coupons"|"populars"|"matches"|"model"|"performance"|"data"|"settings";
const tabs:[TabId,string,string][]=[["today","Bugün","⌂"],["candidates","Günün Adayları","◇"],["coupons","Günün Kuponları","▱"],["populars","Model Destekli Popülerler","☆"],["matches","Maçlar","▣"],["model","Model Performansı","≋"],["performance","Kupon Performansı","↗"],["data","Veri Merkezi","◫"],["settings","Ayarlar","⚙"]];
const pages:Record<Exclude<TabId,"settings">,React.ComponentType>={today:TodayPage,candidates:CandidatesPage,coupons:CouponsPage,populars:PopularsPage,matches:MatchesPage,model:ModelPerformancePage,performance:CouponPerformancePage,data:DataCenterPage};
export default function App(){
 const [tab,setTab]=useState<TabId>(()=>(localStorage.getItem("arz.defaultTab") as TabId)||"today"),[compact,setCompact]=useState(()=>localStorage.getItem("arz.compact")!=="false"),[theme,setTheme]=useState(()=>localStorage.getItem("arz.theme")||"system");
 const startupCheck=localStorage.getItem("arz.startupCheck")!=="false"; const health=useAsync(api.health,[]),readiness=useAsync(api.dataCenterStatus,[],startupCheck),licenseState=useAsync(api.licenseStatus,[]); const [licenseData,setLicenseData]=useState<LicenseStatus|null>(null); const Page=pages[tab as Exclude<TabId,"settings">];
 useEffect(()=>{if(licenseState.data)setLicenseData(licenseState.data)},[licenseState.data]); useEffect(()=>{if(localStorage.getItem("arz.backgroundRefresh")==="false")return;const timer=window.setInterval(readiness.reload,15*60*1000);return()=>window.clearInterval(timer)},[readiness.reload]); useEffect(()=>{const timer=window.setInterval(licenseState.reload,5*60*1000);return()=>window.clearInterval(timer)},[licenseState.reload]); useEffect(()=>{document.documentElement.dataset.theme=theme;localStorage.setItem("arz.theme",theme)},[theme]); useEffect(()=>{document.documentElement.dataset.density=compact?"compact":"comfortable";localStorage.setItem("arz.compact",String(compact))},[compact]);
 const currentLicense=licenseData??{state:"VERIFYING",plan:null,license_id:null,activated_at:null,expires_at:null,last_verified_at:null,device_bound:false,offline_grace_until:null,message:licenseState.error?"Lisans durumu doğrulanamadı.":"Lisans durumu doğrulanıyor."}; const gated=(Boolean(licenseState.error)||(!licenseData&&licenseState.loading)||!['ACTIVE','OFFLINE_GRACE'].includes(currentLicense.state))&&tab!=="settings";
 return <div className="app-shell"><aside className="sidebar"><div className="brand"><BrandLogo/></div><nav aria-label="Ana navigasyon">{tabs.map(([id,label,icon])=><button key={id} className={tab===id?"active":""} onClick={()=>setTab(id)}><span className="nav-icon" aria-hidden="true">{icon}</span><span className="nav-label">{label}</span></button>)}</nav><div className="sidebar-foot"><span className={`health-dot ${health.data?.status==="ok"?"online":"offline"}`}/><div><strong>{health.data?.status==="ok"?"Sistem hazır":"Bağlantı bekleniyor"}</strong><small>{businessDate()}</small></div></div></aside><main className="workspace"><div className="topbar"><span>Yerel ve gizli model analitiği</span><div><LicenseBadge license={currentLicense}/><span className="business-chip">İstanbul · {businessDate()}</span><button className="icon-button" title="Görünüm yoğunluğu" aria-label="Görünüm yoğunluğunu değiştir" onClick={()=>setCompact(v=>!v)}>↕</button></div></div><div className="page">{tab==="settings"?<SettingsPage license={currentLicense} onLicense={v=>setLicenseData(v)}/>:gated?<ActivationPage status={currentLicense} onActivated={v=>setLicenseData(v)}/>:tab==="data"?<><div className="license-diagnostic"><span>Lisans durumu</span><strong>{currentLicense.state}</strong><small>{currentLicense.expires_at?`Bitiş: ${new Date(currentLicense.expires_at).toLocaleDateString("tr-TR")}`:currentLicense.message}</small></div><Page/></>:<Page/>}</div></main><SettingsBridge setTheme={setTheme} setCompact={setCompact}/></div>
}
function SettingsBridge({setTheme,setCompact}:{setTheme:(v:string)=>void;setCompact:(v:boolean)=>void}){useEffect(()=>{const fn=(e:Event)=>{const d=(e as CustomEvent).detail;if(d?.theme)setTheme(d.theme);if(typeof d?.compact==="boolean")setCompact(d.compact)};window.addEventListener("arz-settings",fn);return()=>window.removeEventListener("arz-settings",fn)},[setTheme,setCompact]);return null}
