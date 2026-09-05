import { useCallback, useEffect, useState } from "react";
export function useAsync<T>(loader:()=>Promise<T>, deps:unknown[]=[], enabled=true){
  const [data,setData]=useState<T|null>(null),[loading,setLoading]=useState(true),[error,setError]=useState<string|null>(null);
  const reload=useCallback(()=>{ setLoading(true); setError(null); loader().then(setData).catch(e=>setError(e instanceof Error?e.message:String(e))).finally(()=>setLoading(false)); },deps);
  useEffect(()=>{if(enabled)reload();else setLoading(false)},[reload,enabled]); return {data,loading,error,reload};
}
