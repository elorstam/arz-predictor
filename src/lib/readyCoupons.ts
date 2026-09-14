import type { Coupon } from "../types";

export const couponTypeOrder = ["DAILY_CORNERS", "DAILY_OVER_25", "DAILY_OVER_35", "DAILY_BTTS", "DAILY_HIGH_CONFIDENCE", "DAILY_SURPRISE_SYSTEM", "DAILY_COMPOUND"];
export function readyCoupons(coupons: Coupon[]): Coupon[] {
  return coupons.filter(coupon => coupon.publication_status === "READY")
    .sort((a, b) => couponTypeOrder.indexOf(a.coupon_type) - couponTypeOrder.indexOf(b.coupon_type));
}

// Publication lasts for the business date; settlement does not end visibility.
export function publishedDailyCoupons(coupons: Coupon[], businessDate: string): Coupon[] {
  return coupons.filter(c => c.business_date === businessDate && c.status !== "CANCELLED" && ["READY", "SETTLED"].includes(c.publication_status))
    .sort((a, b) => couponTypeOrder.indexOf(a.coupon_type) - couponTypeOrder.indexOf(b.coupon_type));
}
