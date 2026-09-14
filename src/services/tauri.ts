import { VersionedCache } from "./versionedCache";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { BttsAudit } from "../components/BttsCouponCard";
import type { BulletinStatus, CandidateStatus, CompoundSearch, CompoundSeries, Coupon, CouponPerformance, DailyRun, DataCenterStatus, DatabaseHealth, DatabaseStats, LatestOdd, LineupImpact, LicenseStatus, ModelPerformance, PopularityStatus, PopularResponse, UpcomingMatch, WindowKey } from "../types";

async function call<T>(command:string, args?:Record<string,unknown>):Promise<T> {
  try { return await invoke<T>(command,args); }
  catch (error) { throw new Error(typeof error === "string" ? error : "BACKEND_UNAVAILABLE"); }
}
const performanceCache=new VersionedCache<ModelPerformance>();
let performanceVersion:Promise<number>|null=null;
function sourceVersion(){if(!performanceVersion){performanceVersion=call<number>("model_performance_revision");performanceVersion.finally(()=>{performanceVersion=null}).catch(()=>{});}return performanceVersion;}
let readinessSnapshot:Promise<DataCenterStatus>|null=null;
function refreshReadiness(){readinessSnapshot=call<DataCenterStatus>("data_center_status",{request:{asOf:null}});readinessSnapshot.catch(()=>{readinessSnapshot=null});return readinessSnapshot}

const matchViews=new Map<string,{time:number;value:Promise<UpcomingMatch[]>}>();
// Share only concurrent reads. Every refresh resolves the entire manifest again,
// including category revisions; an earlier empty result is never retained.
const dailyViews=new Map<string,Promise<{run:DailyRun;coupons:Coupon[]}>>();
function dailyOutput(businessDate:string){
 let value=dailyViews.get(businessDate);
 if(!value){
  value=call<{run:DailyRun;coupons:Coupon[]}>("daily_selection_output",{businessDate}).then(output=>{
   output.run.candidates=output.run.candidates.map(c=>({...c,...output.run.selection_sources?.[c.id]}));
   return output;
  });
  dailyViews.set(businessDate,value);
  const current=value;
  void value.finally(()=>{if(dailyViews.get(businessDate)===current)dailyViews.delete(businessDate)}).catch(()=>{});
 }
 return value;
}
function invalidateViews(){matchViews.clear();dailyViews.clear();readinessSnapshot=null;}
async function prepareCurrent(){const result=await call<unknown>("current_pipeline_run");invalidateViews();return result}
function dailyMatches(businessDate:string){let cached=matchViews.get(businessDate);if(!cached||Date.now()-cached.time>60000){if(matchViews.size>=8)matchViews.delete(matchViews.keys().next().value!);const value=call<UpcomingMatch[]>("daily_get_matches",{businessDate});cached={time:Date.now(),value};matchViews.set(businessDate,cached);value.catch(()=>matchViews.delete(businessDate));}return cached.value;}
async function candidates(businessDate:string){return (await dailyOutput(businessDate)).run;}
export const api = {
  bttsLatest:(businessDate:string)=>call<BttsAudit>("btts_pipeline_latest",{businessDate}),
  bttsRefresh:async(businessDate:string)=>{const result=await call<BttsAudit>("btts_pipeline_refresh",{businessDate});invalidateViews();return result;},
  dailyMatches, dailyOutput,
  invalidateDailyViews:invalidateViews,
  dailyResolution:()=>call<Array<{provider_event_id:string;competition:string;competition_id:number;home:string;away:string;kickoff:string;status:string;attempts:number;reason:string|null;candidates:unknown;next_retry_at:string|null}>>("daily_resolution_status"),
  ensureDaily:async(businessDate:string)=>{const result=await call("daily_ensure_publication",{businessDate});invalidateViews();return result;},
  prepareCurrent,
  licenseStatus:()=>call<LicenseStatus>("license_status"), licenseVerify:()=>call<LicenseStatus>("license_verify"), licenseActivate:(licenseKey:string)=>call<LicenseStatus>("license_activate",{request:{licenseKey}}), licenseClearLocalToken:()=>call<void>("license_clear_local_token"),
  health:()=>call<DatabaseHealth>("database_health"), stats:()=>call<DatabaseStats>("database_stats"),
  dataCenterStatus:refreshReadiness,
  currentReadiness:()=>readinessSnapshot??refreshReadiness(),
  refreshIddaa:async()=>{try{const result=await call<unknown>("iddaa_refresh_bulletin");invalidateViews();window.dispatchEvent(new Event("arz-publication-changed"));window.dispatchEvent(new Event("arz-readiness-changed"));return result;}catch(error){invalidateViews();window.dispatchEvent(new Event("arz-readiness-changed"));throw error;}}, refreshPopularity:()=>call<unknown>("iddaa_refresh_popularity"),
  scanLogos:()=>call<unknown>("asset_sync_scan"), startLogoSync:()=>call<boolean>("asset_sync_start"),
  historicalStatus:()=>call<unknown>("football_data_bootstrap_status",{request:{preset:"core_recent_3_seasons"}}),
  bulletinStatus:()=>call<BulletinStatus>("iddaa_bulletin_status"), popularityStatus:()=>call<PopularityStatus>("iddaa_popularity_status"),
  matches:()=>call<UpcomingMatch[]>("iddaa_get_upcoming_matches"), odds:(matchId:number)=>call<LatestOdd[]>("iddaa_get_latest_odds",{request:{matchId}}),
  candidateStatus:()=>call<CandidateStatus>("candidate_engine_status"), candidates,
  coupons:async(businessDate:string)=>{try{return(await dailyOutput(businessDate)).coupons}catch(e){if(e instanceof Error&&e.message==="no candidate run")return call<Coupon[]>("coupon_engine_get_daily",{request:{businessDate,couponType:null}});throw e}}, couponImpact:(couponId:number)=>call<LineupImpact>("coupon_engine_lineup_revision_impact",{request:{couponId}}),
  startCompound:(businessDate:string,startingStakeCents:number)=>call<CompoundSeries>("compound_series_start",{request:{businessDate,startingStakeCents}}),
  generateCompound:(businessDate:string)=>call<Coupon>("compound_series_generate_step",{request:{businessDate,candidateRunId:null,couponType:"DAILY_COMPOUND",unitStakeCents:null}}),
  compoundSearch:(businessDate:string)=>call<CompoundSearch>("compound_search_status",{businessDate}),
  prioritizeMatches:(matchIds:number[])=>call<void>("asset_prioritize_matches",{matchIds}),
  compound:()=>call<CompoundSeries|null>("compound_series_status"),
  coupon:(couponId:number)=>call<Coupon>("coupon_engine_get_coupon",{request:{couponId}}),
  populars:(businessDate:string,supportedOnly=false)=>call<PopularResponse>("model_supported_populars_get",{request:{businessDate,supportedOnly,limit:100}}),
  modelPerformance:async(window:WindowKey,market?:string,context?:string,competitionId?:number)=>{const request={window,market:market||null,competitionId:competitionId||null,predictionContext:context||null,modelVersion:null,calibrationVersion:null,candidateOnly:false,asOf:null};const version=await sourceVersion();return performanceCache.get(JSON.stringify(request),`${version}:${new Date().toLocaleDateString("en-CA",{timeZone:"Europe/Istanbul"})}`,()=>call<ModelPerformance>("model_performance_get",{request}));},
  couponPerformance:(window:WindowKey,couponType?:string)=>call<CouponPerformance>("coupon_performance_get",{request:{window,couponType:couponType||null,asOf:null,includeSystem:true,includeKatlama:true}}),
  lineupStatus:()=>call<Record<string,unknown>>("lineup_engine_status"), lineupModelStatus:()=>call<Record<string,unknown>>("lineup_model_status"), assetStatus:()=>call<Record<string,unknown>>("asset_sync_status"),
  logoPath:(entityType:"TEAM"|"COMPETITION",entityId:number)=>call<string|null>("entity_logo_path",{request:{entityType,entityId}}),
  fileUrl:(path:string)=>convertFileSrc(path),
};

const logoCache=new Map<string,Promise<string|null>>();
export function cachedLogo(entityType:"TEAM"|"COMPETITION",entityId:number){const key=`${entityType}:${entityId}`;if(!logoCache.has(key))logoCache.set(key,api.logoPath(entityType,entityId).then(value=>{if(!value)setTimeout(()=>logoCache.delete(key),15000);return value}).catch(()=>{logoCache.delete(key);return null}));return logoCache.get(key)!}
