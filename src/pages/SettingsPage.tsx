import { useState } from "react";
import { PageHeader, Section } from "../components/Page";
import { UpdateSection } from "../components/UpdateSection";
import { api } from "../services/tauri";
import type { LicenseStatus } from "../types";

export function SettingsPage({ license, onLicense }: { license: LicenseStatus; onLicense: (value: LicenseStatus) => void }) {
  const [theme, setTheme] = useState(localStorage.getItem("arz.theme") || "system");
  const [compact, setCompact] = useState(localStorage.getItem("arz.compact") !== "false");
  const [defaultTab, setDefaultTab] = useState(localStorage.getItem("arz.defaultTab") || "today");
  const [startupCheck, setStartupCheck] = useState(localStorage.getItem("arz.startupCheck") !== "false");
  const [background, setBackground] = useState(localStorage.getItem("arz.backgroundRefresh") !== "false");
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const emit = (detail: object) => window.dispatchEvent(new CustomEvent("arz-settings", { detail }));

  const verify = async () => {
    setBusy(true);
    setError("");
    try { onLicense(await api.licenseVerify()); }
    catch (value) { setError(value instanceof Error ? value.message : "Doğrulama başarısız."); }
    finally { setBusy(false); }
  };

  const activate = async () => {
    setBusy(true);
    setError("");
    try {
      const result = await api.licenseActivate(key);
      setKey("");
      onLicense(result);
    } catch (value) {
      setError(value instanceof Error ? value.message : "Aktivasyon başarısız.");
    } finally {
      setBusy(false);
    }
  };

  const days = remainingDays(license.expires_at);
  return <>
    <PageHeader eyebrow="YEREL TERCİHLER" title="Ayarlar" description="Görünüm, başlangıç ve lisans tercihleri bu cihazda tutulur." />
    <div className="settings-grid">
      <Section title="Lisans">
        <div className="license-summary">
          <div><span>Durum</span><strong>{licenseLabel(license.state)}</strong></div>
          <div><span>Paket</span><strong>{planLabel(license.plan)}</strong></div>
          <div><span>Aktivasyon tarihi</span><strong>{license.activated_at ? new Date(license.activated_at).toLocaleString("tr-TR") : "Henüz yok"}</strong></div>
          <div><span>Bitiş tarihi</span><strong>{license.plan === "LIFETIME" ? "Ömür boyu" : license.expires_at ? new Date(license.expires_at).toLocaleDateString("tr-TR") : "—"}</strong></div>
          <div><span>Kalan süre</span><strong>{license.plan === "LIFETIME" ? "Sınırsız" : days === null ? "—" : `${days} gün`}</strong></div>
          <div><span>Son doğrulama</span><strong>{license.last_verified_at ? new Date(license.last_verified_at).toLocaleString("tr-TR") : "Henüz yok"}</strong></div>
        </div>
        <div className="license-actions">
          <input value={key} onChange={event => setKey(event.target.value)} placeholder="ARZP-XXXX-XXXX-XXXX-XXXX" aria-label="Lisans anahtarı" autoComplete="off" />
          <button className="button" onClick={activate} disabled={busy || !key.trim()}>Aktivasyonu Yap</button>
          <button className="button secondary" onClick={verify} disabled={busy}>Lisansı doğrula</button>
        </div>
        {license.device_bound && <small className="license-bound">Bu lisans bu cihaza bağlıdır.</small>}
        {license.state === "OFFLINE_GRACE" && <small className="license-bound">Çevrimdışı tolerans süresi kullanılıyor.</small>}
        {error && <div className="license-error">{error}</div>}
        <details className="details">
          <summary>Lisans ayrıntıları</summary>
          <p>{license.message}</p>
          {license.offline_grace_until && <p>Çevrimdışı kullanım süresi: {new Date(license.offline_grace_until).toLocaleString("tr-TR")}</p>}
        </details>
      </Section>
      <UpdateSection />
      <Section title="Görünüm">
        <label className="setting">
          <span><strong>Tema</strong><small>Sistem temasını izleyebilir veya sabitleyebilirsiniz.</small></span>
          <select value={theme} onChange={event => { setTheme(event.target.value); emit({ theme: event.target.value }); }}>
            <option value="system">Sistem</option><option value="light">Açık</option><option value="dark">Koyu</option>
          </select>
        </label>
        <Toggle label="Kompakt yoğunluk" note="Daha fazla satırı aynı ekranda gösterir." checked={compact} onChange={value => { setCompact(value); emit({ compact: value }); }} />
      </Section>
      <Section title="Başlangıç">
        <label className="setting">
          <span><strong>Varsayılan sekme</strong><small>ARZ Predictor açıldığında gösterilecek ekran.</small></span>
          <select value={defaultTab} onChange={event => { setDefaultTab(event.target.value); localStorage.setItem("arz.defaultTab", event.target.value); }}>
            <option value="today">Bugün</option><option value="candidates">Günün Adayları</option><option value="coupons">Günün Kuponları</option><option value="populars">Model Destekli Popülerler</option><option value="matches">Maçlar</option><option value="model">Model Performansı</option><option value="performance">Kupon Performansı</option><option value="data">Veri Merkezi</option>
          </select>
        </label>
        <Toggle label="Başlangıç hazırlık kontrolü" note="Uygulama açıldığında yerel hazırlık durumunu kontrol eder." checked={startupCheck} onChange={value => { setStartupCheck(value); localStorage.setItem("arz.startupCheck", String(value)); }} />
        <Toggle label="Arka plan durum kontrolü" note="Hazırlık durumunu 15 dakikada bir, indirme başlatmadan kontrol eder." checked={background} onChange={value => { setBackground(value); localStorage.setItem("arz.backgroundRefresh", String(value)); }} />
      </Section>
      <section className="panel future-panel"><span className="eyebrow">YAKINDA</span><h2>Kurulum kaynakları</h2><p>Yeni veri sağlayıcısı ve lisans alanları yalnız gerçek bir sunucu sözleşmesi oluştuğunda burada yer alacak.</p></section>
    </div>
  </>;
}

function planLabel(value: string | null) {
  return value ? ({ MONTHLY: "Aylık", YEARLY: "Yıllık", LIFETIME: "Ömür Boyu" }[value] ?? value) : "—";
}

function remainingDays(expiresAt: string | null) {
  if (!expiresAt) return null;
  return Math.max(0, Math.ceil((new Date(expiresAt).getTime() - Date.now()) / 86_400_000));
}

function licenseLabel(value: string) {
  return ({ UNLICENSED: "Lisanssız", ACTIVE: "Aktif", OFFLINE_GRACE: "Çevrimdışı tolerans", EXPIRED: "Süresi doldu", SUSPENDED: "Askıya alındı", REVOKED: "İptal edildi", SERVER_UNAVAILABLE: "Sunucu kullanılamıyor", INVALID_TOKEN: "Geçersiz lisans", DEVICE_MISMATCH: "Cihaz eşleşmiyor", CLOCK_ANOMALY: "Saat anomalisi" }[value] ?? value);
}

function Toggle({ label, note, checked, onChange }: { label: string; note: string; checked: boolean; onChange: (value: boolean) => void }) {
  return <label className="setting"><span><strong>{label}</strong><small>{note}</small></span><input type="checkbox" checked={checked} onChange={event => onChange(event.target.checked)} /></label>;
}
