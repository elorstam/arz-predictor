import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";

export type UpdaterFailureKind =
  | "NETWORK"
  | "METADATA"
  | "SIGNATURE"
  | "DOWNLOAD"
  | "INSTALL"
  | "CONFIGURATION"
  | "UNKNOWN";

export interface UpdaterFailure {
  kind: UpdaterFailureKind;
  message: string;
}

export type UpdateChecker = (options: { timeout: number }) => Promise<Update | null>;

export async function installedVersion(): Promise<string> {
  return getVersion();
}

export async function findUpdate(checker: UpdateChecker = check): Promise<Update | null> {
  return checker({ timeout: 15_000 });
}

export function updaterFailure(error: unknown, stage: "check" | "download" | "install" = "check"): UpdaterFailure {
  const detail = error instanceof Error ? error.message : String(error ?? "");
  const normalized = detail.toLocaleLowerCase("en-US");

  if (/signature|minisign|public key|verification/.test(normalized)) {
    return { kind: "SIGNATURE", message: "Güncelleme imzası doğrulanamadı. Paket kurulmadı." };
  }
  if (/json|manifest|metadata|release data|semver|version/.test(normalized)) {
    return { kind: "METADATA", message: "Güncelleme bilgisi geçersiz. Uygulama güvenle çalışmaya devam ediyor." };
  }
  if (/endpoint|configuration|configured/.test(normalized)) {
    return { kind: "CONFIGURATION", message: "Güncelleme kanalı henüz yapılandırılmamış." };
  }
  if (stage === "install") {
    return { kind: "INSTALL", message: "Güncelleme kurulamadı. Mevcut sürüm değişmeden kaldı." };
  }
  if (stage === "download") {
    return { kind: "DOWNLOAD", message: "Güncelleme indirilemedi. Daha sonra yeniden deneyin." };
  }
  if (/network|fetch|connect|dns|timed? ?out|unreachable|http|request/.test(normalized)) {
    return { kind: "NETWORK", message: "Güncelleme sunucusuna ulaşılamadı. Normal kullanıma devam edebilirsiniz." };
  }
  return { kind: "UNKNOWN", message: "Güncelleme kontrolü başarısız. Normal kullanıma devam edebilirsiniz." };
}
