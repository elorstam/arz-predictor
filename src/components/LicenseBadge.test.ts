import { describe, expect, it } from "vitest";
import { licenseBadgeText } from "./LicenseBadge";
import type { LicenseStatus } from "../types";

const devLicense: LicenseStatus = {
  state: "ACTIVE",
  plan: "LIFETIME",
  source: "DEV_BYPASS",
  license_id: null,
  activated_at: null,
  expires_at: null,
  last_verified_at: null,
  device_bound: false,
  offline_grace_until: null,
  message: "development bypass",
};

describe("license badge", () => {
  it("labels a development bypass explicitly instead of imitating a customer license", () => {
    expect(licenseBadgeText(devLicense)).toBe("DEV LICENSE");
  });

  it("keeps the normal lifetime customer label", () => {
    expect(licenseBadgeText({ ...devLicense, source: "SIGNED_TOKEN", license_id: "customer" })).toBe("Ömür Boyu");
  });
});
