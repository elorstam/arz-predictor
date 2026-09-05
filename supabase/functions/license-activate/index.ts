import { hashLicenseKey, jsonError, isExpired, publicLicense, signToken, tokenTimes, type LicenseStatus, type LicensePlan } from "../_shared/license.ts";
import { adminClient, json, signingKey, withCors } from "../_shared/supabase.ts";

function errorDetails(error: unknown) {
  const value = error as { name?: unknown; message?: unknown; stack?: unknown } | null;
  return {
    name: typeof value?.name === "string" ? value.name : undefined,
    message: typeof value?.message === "string" ? value.message : String(error),
    stack: typeof value?.stack === "string" ? value.stack : undefined,
  };
}

Deno.serve(async (req) => {
  const cors = withCors(req); if (cors) return cors;
  try {
    const body = await req.json() as { license_key?: string; device_fingerprint_hash?: string; app_version?: string; token_version?: number };
    if (!body.license_key || !body.device_fingerprint_hash) return jsonError("INVALID_LICENSE_KEY", "Lisans anahtarı ve cihaz bilgisi gerekli.");
    if (body.token_version !== 1) return jsonError("UNSUPPORTED_TOKEN_VERSION", "Desteklenmeyen token sürümü.");
    const client = adminClient();
    const keyHash = await hashLicenseKey(body.license_key);
    const { data: claim, error: claimError } = await client.rpc("claim_license_activation", { p_key_hash: keyHash, p_device_fingerprint_hash: body.device_fingerprint_hash });
    if (claimError) console.error("[license-activate] RPC activation failed", errorDetails(claimError));
    if (claimError) return jsonError("SERVER_ERROR", "Aktivasyon servisi kullanılamıyor.", 500);
    const result = Array.isArray(claim) ? claim[0] : claim;
    const code = result?.result_code as string | undefined;
    const licenseId = result?.license_id as string | undefined;
    if (!licenseId || code !== "ACTIVE") {
      if (licenseId) await client.from("license_audit_log").insert({ license_id: licenseId, action: "ACTIVATION_REJECTED", actor_type: "CLIENT", device_fingerprint_hash: body.device_fingerprint_hash, metadata: { code } });
      const messages: Record<string, string> = { LICENSE_NOT_FOUND: "Lisans bulunamadı.", LICENSE_SUSPENDED: "Lisans askıya alınmış.", LICENSE_REVOKED: "Lisans iptal edilmiş.", LICENSE_EXPIRED: "Lisansın süresi dolmuş.", DEVICE_ALREADY_BOUND: "Lisans başka bir cihaza bağlı." };
      return jsonError(code ?? "LICENSE_NOT_FOUND", messages[code ?? ""] ?? "Aktivasyon reddedildi.");
    }
    const { data: row, error } = await client.from("licenses").select("*").eq("id", licenseId).single();
    if (error) console.error("[license-activate] license lookup failed", errorDetails(error));
    if (error || !row) return jsonError("SERVER_ERROR", "Lisans bilgisi alınamadı.", 500);
    const now = new Date();
    if (isExpired(row.plan as LicensePlan, row.expires_at, now)) return jsonError("LICENSE_EXPIRED", "Lisansın süresi dolmuş.");
    const times = tokenTimes(row.plan as LicensePlan, now);
    const claims = { license_id: row.id, plan: row.plan as LicensePlan, status: row.status as LicenseStatus, device_fingerprint_hash: body.device_fingerprint_hash, device_binding_version: Number(row.device_binding_version), issued_at: now.toISOString(), license_expires_at: row.expires_at, online_revalidate_after: times.onlineRevalidateAfter, offline_grace_until: times.offlineGraceUntil, token_version: 1 as const };
    const token = await signToken(claims, signingKey());
    await client.from("license_audit_log").insert({ license_id: row.id, action: "ACTIVATED", actor_type: "CLIENT", device_fingerprint_hash: body.device_fingerprint_hash, new_status: row.status, metadata: { app_version: body.app_version ?? null } });
    return json({ ok: true, license: publicLicense(row), activation_token: token, server_time: now.toISOString(), online_revalidate_after: times.onlineRevalidateAfter, offline_grace_until: times.offlineGraceUntil });
  } catch (error) {
    console.error("[license-activate] unhandled error", errorDetails(error));
    const code = error instanceof Error ? error.message : "SERVER_ERROR";
    return jsonError(code === "INVALID_LICENSE_KEY" ? code : "SERVER_ERROR", code === "INVALID_LICENSE_KEY" ? "Lisans anahtarı geçersiz." : "Aktivasyon servisi kullanılamıyor.", code === "INVALID_LICENSE_KEY" ? 400 : 500);
  }
});
