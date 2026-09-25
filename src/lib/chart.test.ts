import { describe, expect, it } from "vitest";
import { buildChart } from "./chart";
import type { ServerHistoryPoint } from "./types";

function point(t: number, min: number, avg: number, max: number, cap = 128): ServerHistoryPoint {
  return { t, min, avg, max, cap };
}

describe("buildChart", () => {
  it("refuses to draw a series too short to be a line", () => {
    expect(buildChart([], 300, 56)).toBeNull();
    expect(buildChart([point(0, 1, 1, 1)], 300, 56)).toBeNull();
  });

  it("refuses a zero-sized box", () => {
    const points = [point(0, 0, 0, 0), point(300, 1, 1, 1)];
    expect(buildChart(points, 0, 56)).toBeNull();
    expect(buildChart(points, 300, 0)).toBeNull();
  });

  it("spans the full width and inverts the Y axis", () => {
    const points = [point(0, 0, 0, 0), point(300, 10, 10, 10)];
    const chart = buildChart(points, 300, 56);
    expect(chart).not.toBeNull();
    // Average line: starts bottom-left at the zero bucket, ends top-right at the peak.
    expect(chart?.line).toBe("M0,56L300,0");
  });

  it("closes the min-max band", () => {
    const points = [point(0, 1, 2, 3), point(300, 1, 2, 3)];
    const chart = buildChart(points, 300, 60);
    // Upper edge left-to-right, lower edge back, closed.
    expect(chart?.band).toBe("M0,0L300,0L300,40L0,40Z");
  });

  it("scales to the observed peak, not to capacity", () => {
    // A server peaking at 8 of 128 must not draw as a flat line on the axis.
    const points = [point(0, 0, 4, 8), point(300, 2, 3, 4)];
    const chart = buildChart(points, 300, 100);
    expect(chart?.peak).toBe(8);
    expect(chart?.cap).toBe(128);
    // The 8-player peak reaches the top of the box.
    expect(chart?.line.startsWith("M0,50")).toBe(true);
    expect(chart?.band.startsWith("M0,0")).toBe(true);
  });

  it("survives an all-zero series without dividing by zero", () => {
    const points = [point(0, 0, 0, 0), point(300, 0, 0, 0), point(600, 0, 0, 0)];
    const chart = buildChart(points, 300, 56);
    expect(chart?.peak).toBe(0);
    expect(chart?.line).not.toContain("NaN");
    expect(chart?.band).not.toContain("NaN");
  });

  it("reports the summary figures the legend renders", () => {
    const points = [point(100, 0, 10, 20), point(200, 5, 15, 25), point(300, 1, 2, 3)];
    const chart = buildChart(points, 300, 56);
    expect(chart?.peak).toBe(25);
    expect(chart?.low).toBe(0);
    expect(chart?.mean).toBe(9); // (10 + 15 + 2) / 3
    expect(chart?.from).toBe(100);
    expect(chart?.to).toBe(300);
  });

  it("falls back to zero capacity when no bucket recorded one", () => {
    const points = [point(0, 1, 1, 1, 0), point(300, 1, 1, 1, 0)];
    expect(buildChart(points, 300, 56)?.cap).toBe(0);
  });

  it("emits compact path data", () => {
    const points = Array.from({ length: 288 }, (_, i) => point(i * 300, i % 7, i % 11, i % 13));
    const chart = buildChart(points, 300, 56);
    // Two decimals max, so 288 buckets stay a reasonable DOM string.
    expect(chart?.line).not.toMatch(/\.\d{3}/);
    expect((chart?.line.length ?? 0)).toBeLessThan(4000);
  });
});
