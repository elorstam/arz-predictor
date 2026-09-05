import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { BulletinStatus, CandidateStatus, CompoundSeries, Coupon, CouponPerformance, DailyRun, DataCenterStatus, DatabaseHealth, DatabaseStats, LatestOdd, LineupImpact, LicenseStatus, ModelPerformance, PopularityStatus, PopularResponse, UpcomingMatch, WindowKey } from "../types";

async function call<T>(command:string, args?:Record<string,unknown>):Promise<T> {
  try { return await invoke<T>(command,args); }
  catch (error) { throw new Error(typeof error === "string" ? error : "BACKEND_UNAVAILABLE"); }
}
export const api = {
  licenseStatus:()=>call<LicenseStatus>("license_status"), licenseVerify:()=>call<LicenseStatus>("license_verify"), licenseActivate:(licenseKey:string)=>call<LicenseStatus>("license_activate",{request:{licenseKey}}), licenseClearLocalToken:()=>call<void>("license_clear_local_token"),
  health:()=>call<DatabaseHealth>("database_health"), stats:()=>call<DatabaseStats>("database_stats"),
  dataCenterStatus:()=>call<DataCenterStatus>("data_center_status",{request:{asOf:null}}),
  refreshIddaa:()=>call<unknown>("iddaa_refresh_bulletin"), refreshPopularity:()=>call<unknown>("iddaa_refresh_popularity"),
  scanLogos:()=>call<unknown>("asset_sync_scan"), startLogoSync:()=>call<boolean>("asset_sync_start"),
  historicalStatus:()=>call<unknown>("football_data_bootstrap_status",{request:{preset:"core_recent_3_seasons"}}),
  bulletinStatus:()=>call<BulletinStatus>("iddaa_bulletin_status"), popularityStatus:()=>call<PopularityStatus>("iddaa_popularity_status"),
  matches:()=>call<UpcomingMatch[]>("iddaa_get_upcoming_matches"), odds:(matchId:number)=>call<LatestOdd[]>("iddaa_get_latest_odds",{request:{matchId}}),
  candidateStatus:()=>call<CandidateStatus>("candidate_engine_status"), candidates:(businessDate:string)=>call<DailyRun>("candidate_engine_get_daily",{request:{businessDate,runId:null,category:null}}),
  coupons:(businessDate:string)=>call<Coupon[]>("coupon_engine_get_daily",{request:{businessDate,couponType:null}}), couponImpact:(couponId:number)=>call<LineupImpact>("coupon_engine_lineup_revision_impact",{request:{couponId}}),
  compound:()=>call<CompoundSeries|null>("compound_series_status"),
  populars:(businessDate:string,supportedOnly=false)=>call<PopularResponse>("model_supported_populars_get",{request:{businessDate,supportedOnly,limit:100}}),
  modelPerformance:(window:WindowKey,market?:string,context?:string,competitionId?:number)=>call<ModelPerformance>("model_performance_get",{request:{window,market:market||null,competitionId:competitionId||null,predictionContext:context||null,modelVersion:null,calibrationVersion:null,candidateOnly:false,asOf:null}}),
  couponPerformance:(window:WindowKey,couponType?:string)=>call<CouponPerformance>("coupon_performance_get",{request:{window,couponType:couponType||null,asOf:null,includeSystem:true,includeKatlama:true}}),
  lineupStatus:()=>call<Record<string,unknown>>("lineup_engine_status"), lineupModelStatus:()=>call<Record<string,unknown>>("lineup_model_status"), assetStatus:()=>call<Record<string,unknown>>("asset_sync_status"),
  logoPath:(entityType:"TEAM"|"COMPETITION",entityId:number)=>call<string|null>("entity_logo_path",{request:{entityType,entityId}}),
  fileUrl:(path:string)=>convertFileSrc(path),
};

const logoCache=new Map<string,Promise<string|null>>();
export function cachedLogo(entityType:"TEAM"|"COMPETITION",entityId:number){const key=`${entityType}:${entityId}`;if(!logoCache.has(key))logoCache.set(key,api.logoPath(entityType,entityId));return logoCache.get(key)!}
