import { useEffect, useRef, useState } from "react";
import type { DownloadEvent, Update } from "@tauri-apps/plugin-updater";
import { Section } from "./Page";
import { findUpdate, installedVersion, updaterFailure } from "../services/updater";
import "../update.css";

type Phase = "IDLE" | "CHECKING" | "CURRENT" | "AVAILABLE" | "DOWNLOADING" | "READY" | "INSTALLING" | "FAILED";

const labels: Record<Phase, string> = {
  IDLE: "Henüz kontrol edilmedi",
  CHECKING: "Kontrol ediliyor",
  CURRENT: "Güncel",
  AVAILABLE: "Güncelleme mevcut",
  DOWNLOADING: "İndiriliyor",
  READY: "Kuruluma hazır",
  INSTALLING: "Kuruluyor",
  FAILED: "Güncelleme başarısız",
};

export function UpdateSection() {
  const [version, setVersion] = useState("—");
  const [phase, setPhase] = useState<Phase>("IDLE");
  const [nextVersion, setNextVersion] = useState<string | null>(null);
  const [message, setMessage] = useState("Güncelleme kontrolü normal uygulama başlangıcını veya lisanslamayı engellemez.");
  const [progress, setProgress] = useState<number | null>(null);
  const [lastCheck, setLastCheck] = useState<string | null>(() => localStorage.getItem("arz.updater.lastCheck"));
  const updateRef = useRef<Update | null>(null);

  useEffect(() => {
    installedVersion().then(setVersion).catch(() => setVersion("1.0.0"));
    return () => { void updateRef.current?.close(); };
  }, []);

  const checkNow = async () => {
    setPhase("CHECKING");
    setMessage("Kararlı yayın kanalı kontrol ediliyor…");
    setProgress(null);
    const checkedAt = new Date().toISOString();
    localStorage.setItem("arz.updater.lastCheck", checkedAt);
    setLastCheck(checkedAt);
    try {
      if (updateRef.current) await updateRef.current.close();
      const update = await findUpdate();
      updateRef.current = update;
      if (!update) {
        setPhase("CURRENT");
        setNextVersion(null);
        setMessage("Yüklü sürüm kararlı yayın kanalındaki en güncel sürümdür.");
        return;
      }
      setNextVersion(update.version);
      setPhase("AVAILABLE");
      setMessage("İndirme yalnız onayınızla başlar. Paket imzası kurulumdan önce doğrulanır.");
    } catch (error) {
      const failure = updaterFailure(error);
      setPhase("FAILED");
      setMessage(failure.message);
    }
  };

  const download = async () => {
    const update = updateRef.current;
    if (!update) return;
    setPhase("DOWNLOADING");
    setMessage("İmzalı güncelleme paketi indiriliyor…");
    let received = 0;
    let total: number | undefined;
    try {
      await update.download((event: DownloadEvent) => {
        if (event.event === "Started") total = event.data.contentLength;
        if (event.event === "Progress") received += event.data.chunkLength;
        if (total) setProgress(Math.min(100, Math.round((received / total) * 100)));
      }, { timeout: 120_000 });
      setProgress(100);
      setPhase("READY");
      setMessage("Paket doğrulandı. Kurulum uygulamayı anlaşılır biçimde kapatıp yeniden başlatacaktır.");
    } catch (error) {
      const failure = updaterFailure(error, "download");
      setPhase("FAILED");
      setMessage(failure.message);
    }
  };

  const install = async () => {
    const update = updateRef.current;
    if (!update) return;
    setPhase("INSTALLING");
    setMessage("Güncelleme kuruluyor; ARZ Predictor kısa süre içinde yeniden başlayacak.");
    try {
      await update.install({ restartAfterInstall: true });
    } catch (error) {
      const failure = updaterFailure(error, "install");
      setPhase("FAILED");
      setMessage(failure.message);
    }
  };

  return <Section title="Uygulama Güncellemeleri">
    <div className="update-summary">
      <div><span>Yüklü sürüm</span><strong>{version}</strong></div>
      <div><span>Durum</span><strong className={`update-state update-${phase.toLowerCase()}`}>{labels[phase]}</strong></div>
      {nextVersion&&<div><span>Yeni sürüm</span><strong>{nextVersion}</strong></div>}
      <div><span>Son kontrol</span><strong>{lastCheck ? new Date(lastCheck).toLocaleString("tr-TR") : "Henüz yok"}</strong></div>
    </div>
    {progress!==null&&<div className="update-progress" aria-label={`İndirme yüzde ${progress}`}><i style={{width:`${progress}%`}}/></div>}
    <p className="update-message">{message}</p>
    <div className="update-actions">
      <button className="button secondary" onClick={checkNow} disabled={phase==="CHECKING"||phase==="DOWNLOADING"||phase==="INSTALLING"}>Güncellemeleri Kontrol Et</button>
      {phase==="AVAILABLE"&&<button className="button" onClick={download}>İndir</button>}
      {phase==="READY"&&<button className="button" onClick={install}>Kur ve Yeniden Başlat</button>}
    </div>
  </Section>;
}
