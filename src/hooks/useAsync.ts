import { useCallback, useEffect, useRef, useState } from "react";
import { LatestRequest } from "../lib/latestRequest";
export function useAsync<T>(loader:()=>Promise<T>, deps:unknown[]=[], enabled=true){
  const gate=useRef(new LatestRequest());
  const [state,setState]=useState<{data:T|null;loading:boolean;error:string|null;key:unknown}>({data:null,loading:enabled,error:null,key:null});
  const reload:()=>void=useCallback(()=>{
    const request=gate.current.begin();
    setState({data:null,loading:true,error:null,key:reload});
    Promise.resolve().then(loader).then(data=>{if(gate.current.isCurrent(request))setState({data,loading:false,error:null,key:reload})})
      .catch(e=>{if(gate.current.isCurrent(request))setState({data:null,loading:false,error:e instanceof Error?e.message:String(e),key:reload})});
  },deps);
  useEffect(()=>{if(enabled)reload();return()=>gate.current.cancel()},[reload,enabled]);
  const visible=state.key===reload?state:{data:null,loading:enabled,error:null};
  return {data:visible.data,loading:visible.loading,error:visible.error,reload};
}
