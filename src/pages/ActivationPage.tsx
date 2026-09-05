import { useState } from "react";
import type { LicenseStatus } from "../types";
import { api } from "../services/tauri";

export function ActivationPage({status,onActivated}:{status:LicenseStatus;onActivated:(value:LicenseStatus)=>void}) {
  const [key,setKey] = useState(""); const [error,setError] = useState(""); const [busy,setBusy] = useState(false);
  const activate = async () => { setBusy(true); setError(""); try { const result=await api.licenseActivate(key); setKey(""); onActivated(result); } catch (e) { setError(e instanceof Error ? e.message : "Aktivasyon başarısız."); } finally { setBusy(false); } };
  return <section className="license-gate panel"><div className="license-mark"><img src="/arz-logo-final.png" alt="ARZ"/></div><span className="eyebrow">ARZ PREDICTOR · LİSANS</span><h1>Ürünü etkinleştirin</h1><p>ARZ Predictor kullanımı için bu cihaza bağlı bir lisans anahtarı girin.</p><label>Lisans anahtarı<input value={key} onChange={e=>setKey(e.target.value)} placeholder="ARZP-XXXX-XXXX-XXXX-XXXX" autoComplete="off"/></label><button className="button" onClick={activate} disabled={busy || !key.trim()}>{busy ? "Doğrulanıyor…" : "Aktivasyonu Yap"}</button>{error&&<div className="license-error">{error}</div>}<div className="license-help"><strong>{status.state === "UNLICENSED" ? "Henüz etkinleştirilmedi" : status.state}</strong><span>{status.message}</span></div></section>;
}
