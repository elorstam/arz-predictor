import { authorize, json, jsonError, publicLicense } from "../_shared/admin.ts";

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") return new Response(null, { headers: { "access-control-allow-origin": "*", "access-control-allow-headers": "authorization, content-type", "access-control-allow-methods": "POST, OPTIONS" } });
  try { const { client } = await authorize(req); const body = await req.json().catch(() => ({})) as { search?: string; status?: string; limit?: number; offset?: number }; let query = client.from("licenses").select("*").order("created_at", { ascending: false }).range(body.offset ?? 0, Math.min((body.offset ?? 0) + (body.limit ?? 50) - 1, (body.offset ?? 0) + 99)); if (body.status) query = query.eq("status", body.status); if (body.search) query = query.ilike("id", `%${body.search}%`); const { data, error } = await query; if (error) return jsonError("SERVER_ERROR", "Lisans listesi alınamadı.", 500); return json({ ok: true, licenses: (data ?? []).map(publicLicense) }); }
  catch (error) { const code = error instanceof Error ? error.message : "SERVER_ERROR"; return jsonError(code === "ADMIN_UNAUTHORIZED" ? code : "SERVER_ERROR", code === "ADMIN_UNAUTHORIZED" ? "Yetkisiz yönetici isteği." : "Lisans listesi alınamadı.", code === "ADMIN_UNAUTHORIZED" ? 401 : 500); }
});
