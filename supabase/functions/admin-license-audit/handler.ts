import { jsonError } from "../_shared/license.ts";

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { "content-type": "application/json", "access-control-allow-origin": "*" },
  });
}

type AuditResult = PromiseLike<{ data: Record<string, unknown>[] | null; error: unknown }>;
export type AuditClient = {
  from(table: string): {
    select(fields: string): {
      eq(field: string, value: string): {
        order(field: string, options: { ascending: boolean }): AuditResult;
      };
    };
  };
};

export type AuditAuthorize = (req: Request) => Promise<{ client: AuditClient }>;

const AUDIT_FIELDS = "id,license_id,action,created_at,actor_type,actor_id,old_status,new_status,metadata";
const SAFE_METADATA_FIELDS = ["code", "app_version", "expires_at"] as const;

function safeMetadata(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const source = value as Record<string, unknown>;
  const result: Record<string, unknown> = {};
  for (const field of SAFE_METADATA_FIELDS) {
    const item = source[field];
    if (item === null || typeof item === "string" || typeof item === "number" || typeof item === "boolean") result[field] = item;
  }
  return Object.keys(result).length ? result : null;
}

export function publicAuditRecord(row: Record<string, unknown>) {
  return {
    id: row.id,
    license_id: row.license_id,
    action: row.action,
    created_at: row.created_at,
    actor_type: row.actor_type,
    actor_id: row.actor_id,
    old_status: row.old_status,
    new_status: row.new_status,
    metadata: safeMetadata(row.metadata),
  };
}

export async function handleAuditRequest(req: Request, authorizeRequest: AuditAuthorize): Promise<Response> {
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
    const { client } = await authorizeRequest(req);
    const body = await req.json() as { license_id?: string };
    if (!body.license_id) return jsonError("LICENSE_NOT_FOUND", "Lisans bulunamadı.");

    const { data, error } = await client
      .from("license_audit_log")
      .select(AUDIT_FIELDS)
      .eq("license_id", body.license_id)
      .order("created_at", { ascending: false });
    if (error) return jsonError("SERVER_ERROR", "Denetim geçmişi alınamadı.", 500);

    return json({ ok: true, audit: (data ?? []).map(publicAuditRecord) });
  } catch (error) {
    const code = error instanceof Error ? error.message : "SERVER_ERROR";
    return jsonError(
      code === "ADMIN_UNAUTHORIZED" ? code : "SERVER_ERROR",
      code === "ADMIN_UNAUTHORIZED" ? "Yetkisiz yönetici isteği." : "Denetim geçmişi alınamadı.",
      code === "ADMIN_UNAUTHORIZED" ? 401 : 500,
    );
  }
}
