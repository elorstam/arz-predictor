import { authorize, json, jsonError, publicLicense } from "../_shared/admin.ts";

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") return new Response(null, { headers: { "access-control-allow-origin": "*", "access-control-allow-headers": "authorization, content-type", "access-control-allow-methods": "POST, OPTIONS" } });
  try {
    const { client } = await authorize(req); const body = await req.json() as { license_id?: string };
    if (!body.license_id) return jsonError("LICENSE_NOT_FOUND", "Lisans bulunamadı.");
    const { data, error } = await client.from("licenses").select("*").eq("id", body.license_id).single();
    if (error || !data) return jsonError("LICENSE_NOT_FOUND", "Lisans bulunamadı.");
    return json({ ok: true, license: { ...publicLicense(data), metadata: data.metadata ?? null } });
  } catch (error) { const code = error instanceof Error ? error.message : "SERVER_ERROR"; return jsonError(code === "ADMIN_UNAUTHORIZED" ? code : "SERVER_ERROR", code === "ADMIN_UNAUTHORIZED" ? "Yetkisiz yönetici isteği." : "Lisans alınamadı.", code === "ADMIN_UNAUTHORIZED" ? 401 : 500); }
});
