import { authorize } from "../_shared/admin.ts";
import { handleAuditRequest, type AuditClient } from "./handler.ts";

Deno.serve((req) => handleAuditRequest(req, async (request) => {
  const { client } = await authorize(request);
  return { client: client as unknown as AuditClient };
}));
