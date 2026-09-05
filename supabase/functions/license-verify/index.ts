import { jsonError, isExpired, publicLicense, signToken, tokenTimes, verifyToken, type LicenseStatus, type LicensePlan } from "../_shared/license.ts";
import { adminClient, json, signingKey, withCors } from "../_shared/supabase.ts";

Deno.serve(async (req) => {
  const cors = withCors(req); if (cors) return cors;
  try {
    const body = await req.json() as { activation_token?: string; device_fingerprint_hash?: string; app_version?: string };
    if (!body.activation_token || !body.device_fingerprint_hash) return jsonError("INVALID_TOKEN", "Aktivasyon tokenı ve cihaz bilgisi gerekli.");
    const claims = await verifyToken(body.activation_token, Deno.env.get("ARZ_LICENSE_VERIFYING_KEY_B64") ?? "");
    if (claims.device_fingerprint_hash !== body.device_fingerprint_hash) return jsonError("DEVICE_MISMATCH", "Lisans bu cihaza bağlı değil.");
    const client = adminClient();
    const { data: row, error } = await client.from("licenses").select("*").eq("id", claims.license_id).single();
    if (error || !row) return jsonError("LICENSE_NOT_FOUND", "Lisans bulunamadı.");
    if (Number(row.device_binding_version) !== claims.device_binding_version) return jsonError("INVALID_TOKEN", "Aktivasyon tokenı artık geçerli değil.");
    if (row.device_fingerprint_hash !== body.device_fingerprint_hash) return jsonError("DEVICE_MISMATCH", "Lisans bu cihaza bağlı değil.");
    if (row.status === "SUSPENDED") return jsonError("LICENSE_SUSPENDED", "Lisans askıya alınmış.");
    if (row.status === "REVOKED") return jsonError("LICENSE_REVOKED", "Lisans iptal edilmiş.");
    if (row.status === "EXPIRED") return jsonError("LICENSE_EXPIRED", "Lisansın süresi dolmuş.");
    if (row.status !== "ACTIVE") return jsonError("INVALID_TOKEN", "Lisans etkin değil.");
    const now = new Date();
    if (isExpired(row.plan as LicensePlan, row.expires_at, now)) {
      await client.from("licenses").update({ status: "EXPIRED", updated_at: now.toISOString() }).eq("id", row.id);
      return jsonError("LICENSE_EXPIRED", "Lisansın süresi dolmuş.");
    }
    await client.from("licenses").update({ last_verified_at: now.toISOString(), updated_at: now.toISOString() }).eq("id", row.id);
    const times = tokenTimes(row.plan as LicensePlan, now);
    const token = await signToken({ license_id: row.id, plan: row.plan as LicensePlan, status: row.status as LicenseStatus, device_fingerprint_hash: body.device_fingerprint_hash, device_binding_version: Number(row.device_binding_version), issued_at: now.toISOString(), license_expires_at: row.expires_at, online_revalidate_after: times.onlineRevalidateAfter, offline_grace_until: times.offlineGraceUntil, token_version: 1 }, signingKey());
    await client.from("license_audit_log").insert({ license_id: row.id, action: "VERIFIED", actor_type: "CLIENT", device_fingerprint_hash: body.device_fingerprint_hash, new_status: row.status, metadata: { app_version: body.app_version ?? null } });
    return json({ ok: true, license: publicLicense(row), activation_token: token, server_time: now.toISOString(), online_revalidate_after: times.onlineRevalidateAfter, offline_grace_until: times.offlineGraceUntil });
  } catch (error) {
    const code = error instanceof Error ? error.message : "SERVER_ERROR";
    return jsonError(code === "UNSUPPORTED_TOKEN_VERSION" ? code : "INVALID_TOKEN", code === "UNSUPPORTED_TOKEN_VERSION" ? "Desteklenmeyen token sürümü." : "Aktivasyon tokenı geçersiz.");
  }
});
