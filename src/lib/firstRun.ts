import type { FirstRunStatus } from "../types";
import { readinessLabel } from "./format";

export function isBootstrapping(status: FirstRunStatus | null | undefined): boolean {
  return status?.state !== "COMPLETED";
}

export function readinessTitle(
  status: FirstRunStatus | null | undefined,
  overallStatus: string,
  ready: number,
  total: number,
): string {
  return isBootstrapping(status)
    ? "İlk kurulum hazırlanıyor"
    : `${readinessLabel(overallStatus)} — ${ready}/${total}`;
}
