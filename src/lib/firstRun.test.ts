import { describe, expect, it } from "vitest";
import { isBootstrapping, readinessTitle } from "./firstRun";
import type { FirstRunStatus } from "../types";

const completed = { state: "COMPLETED" } as FirstRunStatus;

describe("first-run readiness labels", () => {
  it("shows preparation instead of action-required while bootstrap is incomplete", () => {
    expect(isBootstrapping({ state: "RUNNING" } as FirstRunStatus)).toBe(true);
    expect(readinessTitle({ state: "RETRY_WAIT" } as FirstRunStatus, "ACTION_REQUIRED", 2, 10))
      .toBe("İlk kurulum hazırlanıyor");
  });

  it("shows the final ten-of-ten ready state after completion", () => {
    expect(isBootstrapping(completed)).toBe(false);
    expect(readinessTitle(completed, "READY", 10, 10)).toBe("HAZIR — 10/10");
  });
});
