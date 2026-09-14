import { expect,it } from "vitest";
import { createBusinessClock,createSharedClock,istanbulDate } from "./businessClock";
it("rolls all subscribers at Istanbul midnight, without restart or duplicate changes",()=>{
 let now=new Date("2026-09-13T20:59:59Z");
 const clock=createBusinessClock(()=>istanbulDate(now));let changes=0;
 const stop=clock.subscribe(()=>changes++);
 expect(clock.snapshot()).toBe("2026-09-13");expect(clock.check()).toBe(false);
 now=new Date("2026-09-13T21:00:00Z");expect(clock.check()).toBe(true);
 expect(clock.snapshot()).toBe("2026-09-14");expect(changes).toBe(1);
 expect(clock.check()).toBe(false);stop();
});
it("catches a suspended window and month/year transitions",()=>{
 let now=new Date("2026-12-31T20:59:00Z");const clock=createBusinessClock(()=>istanbulDate(now));
 now=new Date("2027-01-02T07:00:00Z");clock.check();expect(clock.snapshot()).toBe("2027-01-02");
});
it("uses backend time for all subscribers and rejects stale override responses",()=>{
 const clock=createSharedClock();let changes=0;
 const snap=(time:string,revision:number,overridden=true)=>({utc_ms:Date.parse(time),business_date:istanbulDate(new Date(time)),overridden,revision});
 clock.accept(snap("2026-09-14T23:59:50+03:00",1));
 clock.subscribe(()=>changes++);clock.subscribe(()=>expect(clock.snapshot()).toBe("2026-09-15"));
 expect(clock.accept(snap("2026-09-15T00:00:10+03:00",2))).toBe(true);
 expect(clock.accept(snap("2026-09-15T00:00:10+03:00",2))).toBe(false);
 expect(clock.accept(snap("2026-09-14T23:59:50+03:00",1))).toBe(false);
 expect(changes).toBe(1);
});
