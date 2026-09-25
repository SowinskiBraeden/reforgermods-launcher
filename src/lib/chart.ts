/**
 * Geometry for the population history chart.
 *
 * Pure: it turns history buckets into SVG path strings and nothing else, so the
 * scaling rules are unit-testable without rendering anything.
 *
 * The Y axis scales to the observed peak rather than to slot capacity. A server
 * that peaks at 8 of 128 would otherwise draw as a flat line on the axis; the
 * capacity is stated in the label instead, where it reads better anyway.
 */

import type { ServerHistoryPoint } from "./types";

export interface ChartGeometry {
  /** Filled area between each bucket's min and max. */
  band: string;
  /** Line through each bucket's average. */
  line: string;
  /** Highest `max` in the series, which is the top of the Y scale. */
  peak: number;
  /** Lowest `min` in the series. */
  low: number;
  /** Average of the bucket averages, rounded. */
  mean: number;
  /** Slot capacity, from the last bucket that recorded one. 0 when unknown. */
  cap: number;
  /** First and last bucket timestamps, unix seconds. */
  from: number;
  to: number;
}

/**
 * Builds the chart paths for `points` inside a `width` x `height` box.
 *
 * Returns `null` for a series too short to draw, which the caller renders as an
 * empty state rather than an axis with nothing on it.
 */
export function buildChart(
  points: ServerHistoryPoint[],
  width: number,
  height: number,
): ChartGeometry | null {
  if (points.length < 2 || width <= 0 || height <= 0) return null;

  const peak = points.reduce((acc, p) => Math.max(acc, p.max), 0);
  const low = points.reduce((acc, p) => Math.min(acc, p.min), Number.POSITIVE_INFINITY);
  const meanRaw = points.reduce((acc, p) => acc + p.avg, 0) / points.length;
  const cap = points.reduce((acc, p) => (p.cap > 0 ? p.cap : acc), 0);

  // An all-zero series still needs a finite scale, or every point divides by zero.
  const scaleMax = peak > 0 ? peak : 1;
  const lastIndex = points.length - 1;

  const x = (i: number) => round((i / lastIndex) * width);
  const y = (value: number) => round(height - (value / scaleMax) * height);

  // Upper edge left-to-right, lower edge right-to-left, closed.
  const upper = points.map((p, i) => `${i === 0 ? "M" : "L"}${x(i)},${y(p.max)}`);
  const lower = points
    .slice()
    .reverse()
    .map((p, i) => `L${x(lastIndex - i)},${y(p.min)}`);
  const band = `${upper.join("")}${lower.join("")}Z`;

  const line = points.map((p, i) => `${i === 0 ? "M" : "L"}${x(i)},${y(p.avg)}`).join("");

  return {
    band,
    line,
    peak,
    low: Number.isFinite(low) ? low : 0,
    mean: Math.round(meanRaw),
    cap,
    from: points[0]?.t ?? 0,
    to: points[lastIndex]?.t ?? 0,
  };
}

/** Two decimals is plenty for path data and keeps the DOM string small. */
function round(value: number): number {
  return Math.round(value * 100) / 100;
}
