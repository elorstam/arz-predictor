export type LicensePlan = "MONTHLY" | "YEARLY" | "LIFETIME";
export type LicenseStatus = "UNUSED" | "ACTIVE" | "SUSPENDED" | "REVOKED" | "EXPIRED";
export type ActivationCode = "ACTIVE" | "LICENSE_NOT_FOUND" | "DEVICE_ALREADY_BOUND" | "LICENSE_SUSPENDED" | "LICENSE_REVOKED" | "LICENSE_EXPIRED";

const KEY_PATTERN = /^ARZP([A-Z0-9]{4}){4}$/;
const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

export function normalizeLicenseKey(input: string): string {
  const compact = input.trim().toUpperCase().replace(/[\s-]/g, "");
  if (!KEY_PATTERN.test(compact)) throw new Error("INVALID_LICENSE_KEY");
  return `ARZP-${compact.slice(4, 8)}-${compact.slice(8, 12)}-${compact.slice(12, 16)}-${compact.slice(16, 20)}`;
}

export async function hashLicenseKey(input: string): Promise<string> {
  const normalized = normalizeLicenseKey(input);
  const bytes = new TextEncoder().encode(normalized);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

export function generateLicenseKey(random: Uint8Array = crypto.getRandomValues(new Uint8Array(16))): string {
  let value = "";
  for (const byte of random) value += ALPHABET[byte % ALPHABET.length];
  return `ARZP-${value.slice(0, 4)}-${value.slice(4, 8)}-${value.slice(8, 12)}-${value.slice(12, 16)}`;
}

export function expiryFor(plan: LicensePlan, now: Date, explicit?: string | null): string | null {
  if (plan === "LIFETIME") return null;
  if (explicit) {
    const date = new Date(explicit);
    if (!Number.isNaN(date.valueOf()) && date > now) return date.toISOString();
    throw new Error("INVALID_EXPIRY");
  }
  const result = new Date(now);
  if (plan === "MONTHLY") result.setUTCMonth(result.getUTCMonth() + 1);
  else result.setUTCFullYear(result.getUTCFullYear() + 1);
  return result.toISOString();
}

export function isExpired(plan: LicensePlan, expiresAt: string | null, now = new Date()): boolean {
  return plan !== "LIFETIME" && !!expiresAt && new Date(expiresAt) <= now;
}

export const graceDays: Record<LicensePlan, number> = { MONTHLY: 3, YEARLY: 7, LIFETIME: 14 };

export function tokenTimes(plan: LicensePlan, now: Date): { onlineRevalidateAfter: string; offlineGraceUntil: string } {
  const revalidate = new Date(now.getTime() + 24 * 60 * 60 * 1000);
  const grace = new Date(now.getTime() + graceDays[plan] * 24 * 60 * 60 * 1000);
  return { onlineRevalidateAfter: revalidate.toISOString(), offlineGraceUntil: grace.toISOString() };
}

function b64url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function fromB64url(value: string): Uint8Array {
  const base64 = value.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(value.length / 4) * 4, "=");
  const decoded = atob(base64);
  const bytes = new Uint8Array(decoded.length);
  for (let index = 0; index < decoded.length; index += 1) bytes[index] = decoded.charCodeAt(index);
  return bytes;
}

function toCryptoBytes(input: Uint8Array): Uint8Array<ArrayBuffer> {
  const bytes = new Uint8Array(input.byteLength);
  bytes.set(input);
  return bytes;
}

export type TokenClaims = {
  license_id: string; plan: LicensePlan; status: LicenseStatus; device_fingerprint_hash: string;
  device_binding_version: number; issued_at: string; license_expires_at: string | null;
  online_revalidate_after: string; offline_grace_until: string; token_version: number;
};

export async function signToken(claims: TokenClaims, privateKeyB64: string): Promise<string> {
  const header = b64url(new TextEncoder().encode(JSON.stringify({ alg: "Ed25519", typ: "ARZ-LICENSE", v: 1 })));
  const payload = b64url(new TextEncoder().encode(JSON.stringify(claims)));
  const key = await crypto.subtle.importKey("pkcs8", toCryptoBytes(fromB64url(privateKeyB64)), { name: "Ed25519" }, false, ["sign"]);
  const signature = await crypto.subtle.sign({ name: "Ed25519" }, key, toCryptoBytes(new TextEncoder().encode(`${header}.${payload}`)));
  return `${header}.${payload}.${b64url(new Uint8Array(signature))}`;
}

export async function verifyToken(token: string, publicKeyB64: string): Promise<TokenClaims> {
  const [header, payload, signature] = token.split(".");
  if (!header || !payload || !signature) {
    console.error("[license-verify] invalid token diagnostic", { stage: "token segment validation", error_name: "InvalidToken", error_message: "missing token segment", token_segment_count: token.split(".").length });
    throw new Error("INVALID_TOKEN");
  }

  let parsedHeader: { alg?: string; typ?: string; v?: number };
  try {
    parsedHeader = JSON.parse(new TextDecoder().decode(fromB64url(header))) as { alg?: string; typ?: string; v?: number };
  } catch (error) {
    console.error("[license-verify] invalid token diagnostic", { stage: "header decode", error_name: error instanceof Error ? error.name : "UnknownError", error_message: error instanceof Error ? error.message : String(error), token_segment_count: 3 });
    throw new Error("INVALID_TOKEN");
  }
  if (parsedHeader.alg !== "Ed25519" || parsedHeader.typ !== "ARZ-LICENSE" || parsedHeader.v !== 1) {
    console.error("[license-verify] invalid token diagnostic", { stage: "header claims", error_name: "InvalidToken", error_message: "unsupported token header", token_segment_count: 3 });
    throw new Error("INVALID_TOKEN");
  }

  let publicKeyBytes: Uint8Array<ArrayBuffer>;
  try {
    publicKeyBytes = toCryptoBytes(fromB64url(publicKeyB64));
  } catch (error) {
    console.error("[license-verify] invalid token diagnostic", { stage: "public key decode", error_name: error instanceof Error ? error.name : "UnknownError", error_message: error instanceof Error ? error.message : String(error), key_byte_length: 0, expected_key_format: "Ed25519 public key in SPKI DER, Base64URL-encoded", crypto_import_key_failed: false });
    throw new Error("INVALID_TOKEN");
  }

  let key: CryptoKey;
  try {
    key = await crypto.subtle.importKey("spki", publicKeyBytes, { name: "Ed25519" }, false, ["verify"]);
  } catch (error) {
    console.error("[license-verify] invalid token diagnostic", { stage: "public key import", error_name: error instanceof Error ? error.name : "UnknownError", error_message: error instanceof Error ? error.message : String(error), key_byte_length: publicKeyBytes.byteLength, expected_key_format: "Ed25519 public key in SPKI DER, Base64URL-encoded", crypto_import_key_failed: true });
    throw new Error("INVALID_TOKEN");
  }

  let signatureBytes: Uint8Array<ArrayBuffer>;
  try {
    signatureBytes = toCryptoBytes(fromB64url(signature));
  } catch (error) {
    console.error("[license-verify] invalid token diagnostic", { stage: "signature decode", error_name: error instanceof Error ? error.name : "UnknownError", error_message: error instanceof Error ? error.message : String(error), signature_byte_length: 0, crypto_import_key_failed: false, crypto_verify_returned_false: false });
    throw new Error("INVALID_TOKEN");
  }

  let valid: boolean;
  try {
    valid = await crypto.subtle.verify({ name: "Ed25519" }, key, signatureBytes, toCryptoBytes(new TextEncoder().encode(`${header}.${payload}`)));
  } catch (error) {
    console.error("[license-verify] invalid token diagnostic", { stage: "signature verification", error_name: error instanceof Error ? error.name : "UnknownError", error_message: error instanceof Error ? error.message : String(error), signature_byte_length: signatureBytes.byteLength, crypto_import_key_failed: false, crypto_verify_returned_false: false });
    throw new Error("INVALID_TOKEN");
  }
  if (!valid) {
    console.error("[license-verify] invalid token diagnostic", { stage: "signature verification", error_name: "InvalidSignature", error_message: "crypto.subtle.verify returned false", signature_byte_length: signatureBytes.byteLength, crypto_import_key_failed: false, crypto_verify_returned_false: true });
    throw new Error("INVALID_TOKEN");
  }

  let claims: TokenClaims;
  try {
    claims = JSON.parse(new TextDecoder().decode(fromB64url(payload))) as TokenClaims;
  } catch (error) {
    console.error("[license-verify] invalid token diagnostic", { stage: "claims decode", error_name: error instanceof Error ? error.name : "UnknownError", error_message: error instanceof Error ? error.message : String(error), crypto_import_key_failed: false, crypto_verify_returned_false: false });
    throw new Error("INVALID_TOKEN");
  }
  if (![1].includes(claims.token_version)) {
    console.error("[license-verify] invalid token diagnostic", { stage: "token version", error_name: "UnsupportedTokenVersion", error_message: "unsupported token version", crypto_import_key_failed: false, crypto_verify_returned_false: false });
    throw new Error("UNSUPPORTED_TOKEN_VERSION");
  }
  return claims;
}

export function publicLicense(row: Record<string, unknown>) {
  return { id: row.id, plan: row.plan, status: row.status, created_at: row.created_at, activated_at: row.activated_at, expires_at: row.expires_at, device_bound: Boolean(row.device_fingerprint_hash), device_binding_version: row.device_binding_version, last_verified_at: row.last_verified_at };
}

export function jsonError(code: string, message = code, status = 400) {
  return new Response(JSON.stringify({ ok: false, code, message }), {
    status,
    headers: { "content-type": "application/json", "access-control-allow-origin": "*" },
  });
}
