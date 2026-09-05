import { assertEquals, assertFalse } from "https://deno.land/std@0.224.0/assert/mod.ts";
import { handleAuditRequest, publicAuditRecord } from "../functions/admin-license-audit/handler.ts";

Deno.test("admin audit rejects unauthorized requests before reading a body", async () => {
  const response = await handleAuditRequest(
    new Request("https://example.test/admin-license-audit", { method: "POST" }),
    () => Promise.reject(new Error("ADMIN_UNAUTHORIZED")),
  );
  assertEquals(response.status, 401);
  assertEquals((await response.json()).code, "ADMIN_UNAUTHORIZED");
});

Deno.test("admin audit output uses an explicit safe field allowlist", () => {
  const result = publicAuditRecord({
    id: 1,
    license_id: "license-id",
    action: "GENERATED",
    created_at: "2026-09-05T00:00:00Z",
    actor_type: "ADMIN",
    actor_id: "admin-id",
    old_status: null,
    new_status: "UNUSED",
    metadata: { app_version: "1.0.0", purpose: "İç kullanım", raw_key: "ARZP-BBBB-BBBB-BBBB-BBBB" },
    raw_key: "ARZP-AAAA-BBBB-CCCC-DDDD",
    key_hash: "not-public",
    device_fingerprint_hash: "not-public",
  });
  assertFalse("raw_key" in result);
  assertFalse("key_hash" in result);
  assertFalse("device_fingerprint_hash" in result);
  assertEquals(result.metadata, { app_version: "1.0.0" });
});
