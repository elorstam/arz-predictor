import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { createElement } from "react";
import type { Coupon } from "../types";
import { readyCoupons, publishedDailyCoupons } from "./readyCoupons";

describe("daily usable coupon grid", () => {
  const coupon = (id: number, type: string, state: string) => ({ id, coupon_type: type, publication_status: state } as Coupon);
  it("renders only READY records, in preferred order, without fixed slots", () => {
    const rows = [coupon(1,"DAILY_BTTS","READY"),coupon(2,"DAILY_OVER_35","INSUFFICIENT"),coupon(3,"DAILY_CORNERS","READY"),coupon(4,"DAILY_OVER_25","READY")];
    const visible=readyCoupons(rows);
    expect(visible.map(c=>c.id)).toEqual([3,4,1]);
    const html=renderToStaticMarkup(createElement("div",{className:"coupon-grid"},visible.map(c=>createElement("section",{key:c.id},c.coupon_type))));
    expect(html.match(/<section>/g)).toHaveLength(3);
    expect(html).not.toContain("DAILY_OVER_35");
  });
  it("does not hide READY records or render unavailable states", () => {
    const rows=["INSUFFICIENT","UNAVAILABLE","DRAFT","PENDING_GENERATION","EMPTY"].map((s,i)=>coupon(i,"DAILY_OVER_35",s));
    expect(readyCoupons(rows)).toEqual([]);
    expect(readyCoupons([...rows,coupon(6,"DAILY_CORNERS","READY")])).toHaveLength(1);
  });
  it("keeps published and settled snapshots for their date, including earlier Katlama steps", () => {
    const rows = [coupon(1,"DAILY_OVER_25","READY"),coupon(2,"DAILY_COMPOUND","SETTLED"),coupon(3,"DAILY_COMPOUND","READY"),coupon(4,"DAILY_BTTS","DRAFT")].map(c => ({...c,business_date:"2026-09-02",status:c.publication_status === "SETTLED" ? "SETTLED" : "DRAFT"}));
    expect(publishedDailyCoupons(rows,"2026-09-02").map(c=>c.id)).toEqual([1,2,3]);
    expect(publishedDailyCoupons(rows,"2026-09-03")).toEqual([]);
  });
});
