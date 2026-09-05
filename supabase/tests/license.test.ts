import { assert, assertEquals, assertRejects } from "https://deno.land/std@0.224.0/assert/mod.ts";
import { expiryFor, generateLicenseKey, hashLicenseKey, isExpired, signToken, tokenTimes, verifyToken, type TokenClaims } from "../functions/_shared/license.ts";

function isCryptoKeyPair(value: CryptoKey | CryptoKeyPair): value is CryptoKeyPair {
  return typeof value === "object" && value !== null && "privateKey" in value && "publicKey" in value;
}

function toBase64Url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

Deno.test("license keys normalize and hash consistently", async () => {
  const key = generateLicenseKey(new Uint8Array([1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16]));
  assertEquals(key, "ARZP-1234-5678-9ABC-DEFG");
  assertEquals(await hashLicenseKey(key), await hashLicenseKey(key.toLowerCase().replaceAll("-", " ")));
  await assertRejects(() => hashLicenseKey("ARZP-1234-5678-9ABC"));
});

Deno.test("plans use server clock expiry rules", () => {
  const now = new Date("2026-09-05T12:00:00.000Z");
  assertEquals(expiryFor("MONTHLY", now), "2026-10-05T12:00:00.000Z");
  assertEquals(expiryFor("YEARLY", now), "2027-09-05T12:00:00.000Z");
  assertEquals(expiryFor("LIFETIME", now), null);
  assert(isExpired("MONTHLY", "2026-09-05T12:00:00.000Z", now));
  assert(!isExpired("LIFETIME", null, now));
});

Deno.test("Ed25519 token rejects claim tampering", async () => {
  const generated = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
  if (!isCryptoKeyPair(generated)) throw new Error("Ed25519 key generation did not return a CryptoKeyPair");
  const privateKey = toBase64Url(new Uint8Array(await crypto.subtle.exportKey("pkcs8", generated.privateKey)));
  const publicKey = toBase64Url(new Uint8Array(await crypto.subtle.exportKey("spki", generated.publicKey)));
  const wrongGenerated = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
  if (!isCryptoKeyPair(wrongGenerated)) throw new Error("Ed25519 wrong-key generation did not return a CryptoKeyPair");
  const wrongPublicKey = toBase64Url(new Uint8Array(await crypto.subtle.exportKey("spki", wrongGenerated.publicKey)));
  const times = tokenTimes("YEARLY", new Date());
  const claims: TokenClaims = { license_id: "test", plan: "YEARLY", status: "ACTIVE", device_fingerprint_hash: "device-a", device_binding_version: 0, issued_at: new Date().toISOString(), license_expires_at: null, online_revalidate_after: times.onlineRevalidateAfter, offline_grace_until: times.offlineGraceUntil, token_version: 1 };
  const token = await signToken(claims, privateKey);
  assertEquals((await verifyToken(token, publicKey)).device_fingerprint_hash, "device-a");
  const parts = token.split("."); const changedPayload = btoa(JSON.stringify({ ...claims, device_fingerprint_hash: "device-b" })).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, ""); const changed = `${parts[0]}.${changedPayload}.${parts[2]}`;
  await assertRejects(() => verifyToken(changed, publicKey));
  await assertRejects(() => verifyToken(token, wrongPublicKey));
});
