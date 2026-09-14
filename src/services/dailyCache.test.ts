import { expect, it, vi } from "vitest";
const backend=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock("@tauri-apps/api/core",()=>({invoke:backend.invoke,convertFileSrc:(s:string)=>s}));
import { api } from "./tauri";

it("Candidates and Coupons share an atomic publication; refresh re-reads category revisions",async()=>{
 let revision=80;
 backend.invoke.mockImplementation(async(_command:string,args:{businessDate:string})=>({run:{run_id:63,business_date:args.businessDate,publication_id:`63:${revision}`,candidates:revision===80?[]:[{id:revision}],selection_sources:{}},coupons:[{id:revision}]}));
 const [run,coupons]=await Promise.all([api.candidates("2026-09-13"),api.coupons("2026-09-13")]);
 expect(run.candidates).toHaveLength(0);expect(coupons[0].id).toBe(80);
 expect(backend.invoke).toHaveBeenCalledTimes(1);
 revision=81;
 const updated=await api.candidates("2026-09-13");
 expect(updated.publication_id).toBe("63:81");expect(updated.candidates).toHaveLength(1);
 const [a,b]=await Promise.all([api.candidates("2026-09-12"),api.candidates("2026-09-14")]);
 expect(a.business_date).toBe("2026-09-12");expect(b.business_date).toBe("2026-09-14");
});
