import { useSyncExternalStore } from "react";
import { businessClock } from "../lib/businessClock";
export const useBusinessDate=()=>useSyncExternalStore(businessClock.subscribe,businessClock.snapshot);
