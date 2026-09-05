import { jsonError, publicLicense, type LicensePlan, expiryFor, isExpired } from "./license.ts";
import { adminClient, json, requireAdmin } from "./supabase.ts";

export async function authorize(req: Request) {
  const client = adminClient();
  try { const actorId = await requireAdmin(req, client); return { client, actorId }; }
  catch { throw new Error("ADMIN_UNAUTHORIZED"); }
}

export function plan(value: unknown): LicensePlan {
  if (value !== "MONTHLY" && value !== "YEARLY" && value !== "LIFETIME") throw new Error("INVALID_PLAN");
  return value;
}

export async function audit(client: ReturnType<typeof adminClient>, values: Record<string, unknown>) {
  await client.from("license_audit_log").insert(values);
}

export { json, jsonError, publicLicense, expiryFor, isExpired };
