/**
 * Population history for the selected server.
 *
 * One inline SVG, no charting library: a min–max band, an average line, and a
 * range switcher. The paths come from `lib/chart.ts`, which is pure and tested.
 */

import { historyLoading, historyRange, historySeries, setHistoryRange } from "../lib/state";
import { buildChart } from "../lib/chart";
import { HISTORY_RANGES } from "../lib/types";

/** Drawing box. `preserveAspectRatio="none"` stretches it to the panel width. */
const WIDTH = 300;
const HEIGHT = 56;

export function HistoryChart() {
  const series = historySeries.value;
  const loading = historyLoading.value;
  const range = historyRange.value;
  const geometry = series ? buildChart(series.points, WIDTH, HEIGHT) : null;

  return (
    <div class="section">
      <div class="section__h">
        <span>Population</span>
        <div class="rangeswitch">
          {HISTORY_RANGES.map((option) => (
            <button
              key={option.value}
              data-on={option.value === range}
              onClick={() => setHistoryRange(option.value)}
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>

      {geometry ? (
        <>
          <svg
            class="chart"
            viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
            preserveAspectRatio="none"
            role="img"
            aria-label={`Population over the last ${range}: peak ${geometry.peak}, average ${geometry.mean}`}
          >
            {/* Band first so the average line sits on top of it. */}
            <path class="chart__band" d={geometry.band} />
            {/* non-scaling-stroke keeps the line 1px despite the stretched viewBox. */}
            <path class="chart__line" d={geometry.line} vector-effect="non-scaling-stroke" />
          </svg>
          <div class="chart__legend">
            <span>
              peak <b>{geometry.peak}</b>
            </span>
            <span>
              avg <b>{geometry.mean}</b>
            </span>
            <span>
              low <b>{geometry.low}</b>
            </span>
            {geometry.cap > 0 && <span>of {geometry.cap}</span>}
            <span class="chart__bucket">{series?.bucket} buckets</span>
          </div>
        </>
      ) : (
        <p class="state__hint" style={{ margin: 0 }}>
          {loading ? "Loading history…" : "No recorded history for this range."}
        </p>
      )}
    </div>
  );
}
