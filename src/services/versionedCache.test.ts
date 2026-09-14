import {expect,it,vi} from "vitest";
import {VersionedCache} from "./versionedCache";
it("coalesces duplicate invokes and invalidates on settled source revision",async()=>{
 const cache=new VersionedCache<number>(),load=vi.fn().mockResolvedValue(7);
 expect(await Promise.all([cache.get("30:all:BASE:1","1",load),cache.get("30:all:BASE:1","1",load)])).toEqual([7,7]);
 expect(load).toHaveBeenCalledTimes(1);
 await cache.get("30:all:BASE:1","1",load);expect(load).toHaveBeenCalledTimes(1);
 await cache.get("30:all:BASE:1","2",load);expect(load).toHaveBeenCalledTimes(2);
});
it("rapid filters retain separate responses even when they finish out of order",async()=>{
 const cache=new VersionedCache<string>();let finish!:(v:string)=>void;
 const a=cache.get("7:BTTS","1",()=>new Promise(resolve=>finish=resolve));
 await Promise.resolve();
 expect(await cache.get("30:all","1",async()=>"all")).toBe("all");
 finish("BTTS");expect(await a).toBe("BTTS");
 expect(await cache.get("30:all","1",async()=>"wrong")).toBe("all");
});
