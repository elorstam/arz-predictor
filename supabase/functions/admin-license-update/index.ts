import { authorize, audit, isExpired, json, jsonError } from "../_shared/admin.ts";
import { publicLicense } from "../_shared/license.ts";

type AdminAction = "SUSPEND" | "REACTIVATE" | "REVOKE" | "EXTEND";

function adminAction(value: unknown): AdminAction {
  if (value !== "SUSPEND" && value !== "REACTIVATE" && value !== "REVOKE" && value !== "EXTEND") {
    throw new Error("INVALID_ACTION");
  }
  return value;
}

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") {
    return new Response(null, {
      headers: {
        "access-control-allow-origin": "*",
        "access-control-allow-headers": "authorization, content-type",
        "access-control-allow-methods": "POST, OPTIONS",
      },
    });
  }

  try {
    const { client, actorId } = await authorize(req);
    const body = await req.json() as {
      license_id?: string;
      action?: unknown;
      expires_at?: string;
      days?: number;
      months?: number;
      years?: number;
    };
    if (!body.license_id || !body.action) return jsonError("INVALID_REQUEST", "Lisans ve işlem gerekli.");
    const action = adminAction(body.action);

    const { data: old, error: fetchError } = await client.from("licenses").select("*").eq("id", body.license_id).single();
    if (fetchError || !old) return jsonError("LICENSE_NOT_FOUND", "Lisans bulunamadı.");

    const now = new Date();
    let nextStatus = String(old.status);
    let update: Record<string, unknown> = { updated_at: now.toISOString() };

    if (action === "SUSPEND") {
      if (old.status === "REVOKED") return jsonError("LICENSE_REVOKED", "İptal edilmiş lisans askıya alınamaz.");
      nextStatus = "SUSPENDED";
      update = { ...update, status: nextStatus, suspended_at: now.toISOString() };
    }

    if (action === "REACTIVATE") {
      if (old.status !== "SUSPENDED") return jsonError("INVALID_STATUS", "Yalnız askıya alınmış lisans yeniden etkinleştirilebilir.");
      if (isExpired(old.plan, old.expires_at, now)) return jsonError("LICENSE_EXPIRED", "Süresi dolmuş lisans yeniden etkinleştirilemez.");
      nextStatus = old.device_fingerprint_hash ? "ACTIVE" : "UNUSED";
      update = { ...update, status: nextStatus, suspended_at: null };
    }

    if (action === "REVOKE") {
      nextStatus = "REVOKED";
      update = { ...update, status: nextStatus, revoked_at: now.toISOString() };
    }

    if (action === "EXTEND") {
      if (old.plan === "LIFETIME") return jsonError("LIFETIME_HAS_NO_EXPIRY", "Ömür boyu lisansın bitiş tarihi yoktur.");
      const current = old.expires_at && new Date(old.expires_at) > now ? new Date(old.expires_at) : now;
      let expires: Date;
      if (body.expires_at) {
        expires = new Date(body.expires_at);
      } else {
        expires = new Date(current);
        expires.setUTCDate(expires.getUTCDate() + (body.days ?? 0));
        expires.setUTCMonth(expires.getUTCMonth() + (body.months ?? 0));
        expires.setUTCFullYear(expires.getUTCFullYear() + (body.years ?? 0));
      }
      if (!(expires > current)) return jsonError("INVALID_EXPIRY", "Yeni bitiş tarihi lisansı kısaltamaz.");
      nextStatus = old.status === "EXPIRED" ? (old.device_fingerprint_hash ? "ACTIVE" : "UNUSED") : String(old.status);
      update = { ...update, expires_at: expires.toISOString(), status: nextStatus };
    }

    const { data: row, error } = await client.from("licenses").update(update).eq("id", old.id).select("*").single();
    if (error || !row) return jsonError("SERVER_ERROR", "Lisans güncellenemedi.", 500);

    const auditAction = action === "SUSPEND"
      ? "SUSPENDED"
      : action === "REACTIVATE"
      ? "REACTIVATED"
      : action === "REVOKE"
      ? "REVOKED"
      : "EXTENDED";
    await audit(client, {
      license_id: row.id,
      action: auditAction,
      actor_type: "ADMIN",
      actor_id: actorId,
      old_status: old.status,
      new_status: nextStatus,
      metadata: { expires_at: row.expires_at },
    });
    return json({ ok: true, license: publicLicense(row) });
  } catch (error) {
    const code = error instanceof Error ? error.message : "SERVER_ERROR";
    if (code === "ADMIN_UNAUTHORIZED") return jsonError(code, "Yetkisiz yönetici isteği.", 401);
    if (code === "INVALID_ACTION") return jsonError(code, "Desteklenmeyen yönetici işlemi.");
    return jsonError("SERVER_ERROR", "Lisans güncellenemedi.", 500);
  }
});
