import {expect,it} from "vitest";
import type {Candidate,DailyRun} from "../types";
import {dailySelections,selectedContext} from "./dailySelections";
it("All deduplicates event market outcome line while category membership is retained",()=>{
 const row={id:1,match_id:10,market:"TOTAL_GOALS",selection:"OVER",line:2.5,category:"OVER_25"} as Candidate;
 const run={candidates:[row,{...row,id:2,category:"HIGH_CONFIDENCE"},{...row,id:3,line:3.5},{...row,id:4,selection:"UNDER"}]} as DailyRun;
 expect(dailySelections(run,"TÜMÜ")).toHaveLength(3);
 expect(dailySelections(run,"HIGH_CONFIDENCE").map(x=>x.id)).toEqual([2]);
 expect(dailySelections(run,"BTTS_YES")).toHaveLength(0);
 expect(dailySelections(run,"TÜMÜ")).toHaveLength(3);
});
it("lineup text requires a selected and backend verified authoritative revision",()=>{
 expect(selectedContext({prediction_context:"LINEUP_AWARE"} as DailyRun)).toBe("BASE");
 expect(selectedContext({prediction_context:"LINEUP_AWARE",lineup_verified:false} as DailyRun)).toBe("BASE");
 expect(selectedContext({prediction_context:"BASE",lineup_verified:true} as DailyRun)).toBe("BASE");
 expect(selectedContext({prediction_context:"LINEUP_AWARE",lineup_verified:true} as DailyRun)).toBe("LINEUP_AWARE");
});
