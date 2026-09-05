import { audit, authorize, expiryFor, json, jsonError, plan } from "../_shared/admin.ts";
import { generateLicenseKey, hashLicenseKey } from "../_shared/license.ts";

Deno.serve(async (req) => {
  const cors = req.method === "OPTIONS" ? new Response(null, { headers: { "access-control-allow-origin": "*", "access-control-allow-headers": "apikey, authorization, content-type, x-client-info", "access-control-allow-methods": "POST, OPTIONS" } }) : null; if (cors) return cors;
  try {
    const { client, actorId } = await authorize(req); const body = await req.json() as { plan?: string; expires_at?: string | null; metadata?: Record<string, unknown> };
    const selectedPlan = plan(body.plan); const rawKey = generateLicenseKey(); const keyHash = await hashLicenseKey(rawKey); const now = new Date(); const expiresAt = expiryFor(selectedPlan, now, body.expires_at);
    const { data: row, error } = await client.from("licenses").insert({ key_hash: keyHash, plan: selectedPlan, status: "UNUSED", expires_at: expiresAt, metadata: body.metadata ?? null }).select("*").single();
    if (error || !row) return jsonError("SERVER_ERROR", "Lisans oluşturulamadı.", 500);
    await audit(client, { license_id: row.id, action: "GENERATED", actor_type: "ADMIN", actor_id: actorId, new_status: "UNUSED" });
    return json({ ok: true, license: { id: row.id, raw_key: rawKey, plan: row.plan, status: row.status, created_at: row.created_at, expires_at: row.expires_at } });
  } catch (error) { const code = error instanceof Error ? error.message : "SERVER_ERROR"; return jsonError(code === "ADMIN_UNAUTHORIZED" ? code : code === "INVALID_PLAN" || code === "INVALID_EXPIRY" ? code : "SERVER_ERROR", code === "ADMIN_UNAUTHORIZED" ? "Yetkisiz yönetici isteği." : "Lisans oluşturulamadı.", code === "ADMIN_UNAUTHORIZED" ? 401 : 400); }
});
