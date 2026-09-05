import { describe, expect, it, vi } from "vitest";
import { findUpdate, updaterFailure, type UpdateChecker } from "./updater";

describe("production updater orchestration", () => {
  it("detects a newer version returned by the signed native updater", async () => {
    const candidate = { currentVersion: "0.9.9", version: "1.0.0" };
    const checker = vi.fn().mockResolvedValue(candidate) as unknown as UpdateChecker;
    await expect(findUpdate(checker)).resolves.toBe(candidate);
    expect(checker).toHaveBeenCalledWith({ timeout: 15_000 });
  });

  it("treats same or older versions as no update when the native updater returns null", async () => {
    const checker = vi.fn().mockResolvedValue(null) as unknown as UpdateChecker;
    await expect(findUpdate(checker)).resolves.toBeNull();
  });

  it("keeps an unreachable endpoint non-blocking", () => {
    expect(updaterFailure(new Error("network request timed out"))).toMatchObject({ kind: "NETWORK" });
  });

  it("fails safely on malformed metadata", () => {
    expect(updaterFailure(new Error("manifest JSON is invalid"))).toMatchObject({ kind: "METADATA" });
  });

  it("surfaces native signature rejection without allowing installation", () => {
    expect(updaterFailure(new Error("minisign signature verification failed"))).toMatchObject({ kind: "SIGNATURE" });
  });

  it("reports download and installation failures without changing the current app", () => {
    expect(updaterFailure(new Error("stream closed"), "download").kind).toBe("DOWNLOAD");
    expect(updaterFailure(new Error("installer failed"), "install").kind).toBe("INSTALL");
  });
});
