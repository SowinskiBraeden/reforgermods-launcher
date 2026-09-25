/**
 * Table ordering.
 *
 * Two kinds of sort exist here, and the difference is visible to the user:
 *
 * - **Dataset-wide.** `/v2/servers` orders every matching server before
 *   paginating, so the first page really is the top of the list. It accepts
 *   `players`, `name`, `newest` and `lastSeen`, and no direction parameter.
 * - **Page-local.** Everything else — queue, ping, mod count, region, and the
 *   reverse of the two directions the API does offer — can only reorder the
 *   hundred rows already loaded. Sorting by queue does *not* find the most
 *   queued servers in the fleet; it reorders the page the API chose.
 *
 * The distinction is kept in the types rather than in a comment in the UI,
 * because presenting the second as though it were the first is the kind of
 * quiet lie this codebase avoids elsewhere (see the "Filters the loaded page"
 * label on minimum players).
 *
 * These functions are pure and take latency as a lookup, so they can be tested
 * without the signal graph or a Tauri host.
 */

import { humaniseRegion, tidyName } from "./format";
import type { ServerSort, ServerSummary } from "./types";

/** A column the table can be ordered by. */
export type SortColumn = "name" | "players" | "queue" | "ping" | "mods" | "region";

export type SortDirection = "asc" | "desc";

export interface TableSort {
  column: SortColumn;
  direction: SortDirection;
}

/** Resolves a region's median latency, or null when it was never measured. */
export type PingLookup = (pingSiteId: string | null) => number | null;

/**
 * The direction a column takes on its first click.
 *
 * Chosen so one click gives the answer people actually want: the busiest
 * servers, the lowest ping, the most mods — but names from A.
 */
export const NATURAL_DIRECTION: Record<SortColumn, SortDirection> = {
  name: "asc",
  players: "desc",
  queue: "desc",
  ping: "asc",
  mods: "desc",
  region: "asc",
};

/**
 * Columns the API can order the whole dataset by, with the direction it returns.
 *
 * There is no direction parameter, so only these exact pairings are
 * dataset-wide; anything else falls back to reordering the loaded page.
 */
export const API_SORTABLE: Partial<
  Record<SortColumn, { sort: ServerSort; direction: SortDirection }>
> = {
  name: { sort: "name", direction: "asc" },
  players: { sort: "players", direction: "desc" },
};

/** Column headings, also used in the note naming the active page-local sort. */
export const COLUMN_LABEL: Record<SortColumn, string> = {
  name: "server",
  players: "players",
  queue: "queue",
  ping: "ping",
  mods: "mods",
  region: "region",
};

/**
 * The sort a click on `column` produces.
 *
 * Clicking the active column reverses it; clicking any other starts at that
 * column's natural direction rather than inheriting the previous one, which
 * would otherwise sort names Z–A just because players had been sorted high–low.
 */
export function nextSort(current: TableSort | null, column: SortColumn): TableSort {
  if (current?.column === column) {
    return { column, direction: current.direction === "asc" ? "desc" : "asc" };
  }
  return { column, direction: NATURAL_DIRECTION[column] };
}

/** True when the API can serve this ordering across the whole dataset. */
export function isDatasetWide(sort: TableSort): boolean {
  const api = API_SORTABLE[sort.column];
  return api !== undefined && api.direction === sort.direction;
}

/**
 * Orders `rows` by `sort`.
 *
 * Returns the input untouched when there is nothing to do, so the browse view
 * keeps the API's ordering — and its array identity — rather than paying for a
 * copy and a re-render on every refresh.
 */
export function sortRows(
  rows: ServerSummary[],
  sort: TableSort | null,
  pingOf: PingLookup,
): ServerSummary[] {
  if (!sort) return rows;
  const direction = sort.direction === "asc" ? 1 : -1;

  return [...rows].sort((a, b) => {
    if (sort.column === "ping") {
      const pa = pingOf(a.pingSiteId);
      const pb = pingOf(b.pingSiteId);
      // Unmeasured regions sort last in *both* directions. Flipping them with
      // the rest would present "never measured" as the fastest thing on the
      // page, which is the one reading that is actively misleading.
      if (pa === null && pb === null) return tieBreak(a, b);
      if (pa === null) return 1;
      if (pb === null) return -1;
      return (pa - pb) * direction || tieBreak(a, b);
    }
    return compareAscending(a, b, sort.column) * direction || tieBreak(a, b);
  });
}

/** Compares two rows on `column`, always ascending. */
function compareAscending(
  a: ServerSummary,
  b: ServerSummary,
  column: Exclude<SortColumn, "ping">,
): number {
  switch (column) {
    case "name":
      return tidyName(a.name).localeCompare(tidyName(b.name));
    case "players":
      return a.players - b.players;
    case "queue":
      return a.queue - b.queue;
    case "mods":
      return a.modCount - b.modCount;
    case "region":
      // Sorted by the label shown, not the raw id, so the order matches what is
      // on screen — "New York" sits under N, not under "new_york".
      return humaniseRegion(a.pingSiteId ?? a.region).localeCompare(
        humaniseRegion(b.pingSiteId ?? b.region),
      );
  }
}

/**
 * Settles ties deterministically.
 *
 * Without this, equal values (every vanilla server has zero mods) would reorder
 * themselves on each refresh, because `Array.prototype.sort` is only stable
 * with respect to the array it was given and that array is rebuilt per request.
 */
function tieBreak(a: ServerSummary, b: ServerSummary): number {
  return b.players - a.players || tidyName(a.name).localeCompare(tidyName(b.name));
}
