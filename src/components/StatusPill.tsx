/**
 * Compact API status for the header.
 *
 * Reads `nowMs`, which ticks once a second, so the "updated Ns ago" figure stays
 * live. Because signals subscribe per component, that tick re-renders this pill
 * and nothing else — the server table is untouched.
 */

import { nowMs, status } from "../lib/state";
import { formatAge } from "../lib/format";

export function StatusPill() {
  const s = status.value;
  // Subscribe to the clock so the age figure advances.
  const now = nowMs.value;

  if (!s) {
    return (
      <div class="status" title="Checking the reforgermods.net API.">
        <span class="status__dot" />
        <span>Checking API…</span>
      </div>
    );
  }

  if (!s.apiReachable) {
    return (
      <div class="status" title={s.note ?? "The reforgermods.net API did not respond."}>
        <span class="status__dot status__dot--bad" />
        <span>API unreachable</span>
      </div>
    );
  }

  const dataset = s.dataset;
  // `snapshotAgeSeconds` was measured when the response was produced; advance it
  // by the time since, so the figure keeps counting between requests.
  const measuredAtMs = datasetMeasuredAt(dataset?.snapshotAgeSeconds);
  const age =
    dataset && measuredAtMs !== null
      ? dataset.snapshotAgeSeconds + (now - measuredAtMs) / 1000
      : null;

  const stale = dataset?.stale === true;
  const warming = dataset?.warming === true;
  const dotClass = warming || stale ? "status__dot--warn" : "status__dot--ok";

  return (
    <div class="status" title={s.note ?? undefined}>
      <span class={`status__dot ${dotClass}`} />
      <span>{s.reportedStatus ? `API ${s.reportedStatus}` : "API operational"}</span>
      {dataset && (
        <>
          <span class="status__divider" />
          <span>
            {warming
              ? "Data warming up"
              : age === null
                ? "Data age unknown"
                : `Updated ${formatAge(age)}`}
          </span>
        </>
      )}
    </div>
  );
}

/**
 * Wall-clock time the current `snapshotAgeSeconds` reading was taken.
 *
 * Tracked here rather than in state because it exists only to make the label
 * tick; it is reset whenever the reading changes.
 */
let lastAgeSeconds: number | null = null;
let lastAgeAtMs: number | null = null;

function datasetMeasuredAt(ageSeconds: number | undefined): number | null {
  if (ageSeconds === undefined) return null;
  if (ageSeconds !== lastAgeSeconds) {
    lastAgeSeconds = ageSeconds;
    lastAgeAtMs = Date.now();
  }
  return lastAgeAtMs;
}
