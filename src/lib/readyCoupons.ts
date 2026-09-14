import type { Coupon } from "../types";

export const couponTypeOrder = ["DAILY_CORNERS", "DAILY_OVER_25", "DAILY_OVER_35", "DAILY_BTTS", "DAILY_HIGH_CONFIDENCE", "DAILY_SURPRISE_SYSTEM", "DAILY_COMPOUND"];
export function readyCoupons(coupons: Coupon[]): Coupon[] {
  return coupons.filter(coupon => coupon.publication_status === "READY")
    .sort((a, b) => couponTypeOrder.indexOf(a.coupon_type) - couponTypeOrder.indexOf(b.coupon_type));
}
