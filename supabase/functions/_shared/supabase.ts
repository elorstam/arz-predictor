import { createClient, type SupabaseClient } from "https://esm.sh/@supabase/supabase-js@2.49.1";

export function adminClient(): SupabaseClient {
  const url = Deno.env.get("SUPABASE_URL");
  const key = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");
  if (!url || !key) throw new Error("SERVER_CONFIGURATION_ERROR");
  return createClient(url, key, { auth: { autoRefreshToken: false, persistSession: false } });
}

export async function requireAdmin(req: Request, client: SupabaseClient): Promise<string> {
  const token = req.headers.get("authorization")?.replace(/^Bearer\s+/i, "");
  if (!token) throw new Error("ADMIN_UNAUTHORIZED");
  const { data, error } = await client.auth.getUser(token);
  if (error || !data.user || data.user.app_metadata?.role !== "admin") throw new Error("ADMIN_UNAUTHORIZED");
  return data.user.id;
}

export function signingKey(): string {
  const key = Deno.env.get("ARZ_LICENSE_SIGNING_KEY_B64");
  if (!key) throw new Error("SERVER_CONFIGURATION_ERROR");
  return key;
}

export function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), { status, headers: { "content-type": "application/json", "access-control-allow-origin": "*" } });
}

export function withCors(req: Request): Response | null {
  if (req.method === "OPTIONS") return new Response(null, { headers: { "access-control-allow-origin": "*", "access-control-allow-headers": "authorization, content-type", "access-control-allow-methods": "POST, OPTIONS" } });
  return null;
}
