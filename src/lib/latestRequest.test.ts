import {expect,it} from 'vitest';
import {LatestRequest} from './latestRequest';
it('a late empty response from the previous date cannot replace the selected date',async()=>{
 const gate=new LatestRequest();let visible:number[]=[];
 let finishOld!:(value:number[])=>void;
 const old=new Promise<number[]>(r=>{finishOld=r});
 const a=gate.begin();const first=old.then(v=>{if(gate.isCurrent(a))visible=v});
 const b=gate.begin();await Promise.resolve([13,14]).then(v=>{if(gate.isCurrent(b))visible=v});
 finishOld([]);await first;expect(visible).toEqual([13,14]);
});
it('unmount invalidates pending results',()=>{
 const gate=new LatestRequest();const request=gate.begin();gate.cancel();expect(gate.isCurrent(request)).toBe(false);
});
