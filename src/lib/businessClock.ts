export const istanbulDate=(now=new Date())=>new Intl.DateTimeFormat("en-CA",{timeZone:"Europe/Istanbul",year:"numeric",month:"2-digit",day:"2-digit"}).format(now);

// One clock for every daily view. Focus catches suspended/background windows.
export function createBusinessClock(read=()=>istanbulDate()) {
 let date=read();
 const listeners=new Set<()=>void>();
 return {
  snapshot:()=>date,
  subscribe:(listener:()=>void)=>{listeners.add(listener);return()=>{listeners.delete(listener)}},
  check:()=>{const next=read();if(next===date)return false;date=next;listeners.forEach(fn=>fn());return true},
 };
}
export type BusinessClockSnapshot={utc_ms:number;business_date:string;overridden:boolean;revision:number};
export function createSharedClock(){
 let source:BusinessClockSnapshot|null=null,received=0;
 const now=()=>new Date(source?source.utc_ms+(source.overridden?0:performance.now()-received):Date.now());
 const clock=createBusinessClock(()=>istanbulDate(now()));
 return {...clock,now,accept:(snapshot:BusinessClockSnapshot)=>{
  if(source&&snapshot.revision<source.revision)return false;
  source=snapshot;received=performance.now();return clock.check();
 }};
}
export const businessClock=createSharedClock();
