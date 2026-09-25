/**
 * Table ordering.
 *
 * The cases that matter here are the ones where a plausible implementation is
 * quietly wrong: unmeasured latency presented as fast, a direction inherited
 * from the previous column, or equal values reshuffling on every refresh.
 */

import { describe, expect, it } from "vitest";
import {
  API_SORTABLE,
  isDatasetWide,
  nextSort,
  sortRows,
  type PingLookup,
  type TableSort,
} from "./sort";
import type { ServerSummary } from "./types";

function server(overrides: Partial<ServerSummary> & { id: string }): ServerSummary {
  return {
    name: `server ${overrides.id}`,
    scenarioId: "{ABC}Missions/Conflict.conf",
    scenarioName: "Conflict",
    scenarioThumbnailUrl: null,
    gameVersion: "1.8.0.13",
    official: false,
    passwordProtected: false,
    battlEye: true,
    crossPlay: true,
    players: 0,
    maxPlayers: 128,
    queue: 0,
    queueMax: 25,
    pingSiteId: null,
    region: "new_york",
    platform: "LINUX",
    platforms: ["pc"],
    modCount: 0,
    online: true,
    firstSeen: null,
    lastSeen: null,
    analytics: null,
    joinUrl: null,
    ...overrides,
  };
}

/** No region was ever measured. */
const noPings: PingLookup = () => null;

const pings: PingLookup = (id) =>
  ({ frankfurt: 22, new_york: 95, sydney: 280 })[id ?? ""] ?? null;

const ids = (rows: ServerSummary[]) => rows.map((r) => r.id);

describe("nextSort", () => {
  it("starts a column at the direction that answers the obvious question", () => {
    // Busiest, most modded and lowest ping first; names from A.
    expect(nextSort(null, "players")).toEqual({ column: "players", direction: "desc" });
    expect(nextSort(null, "mods")).toEqual({ column: "mods", direction: "desc" });
    expect(nextSort(null, "queue")).toEqual({ column: "queue", direction: "desc" });
    expect(nextSort(null, "ping")).toEqual({ column: "ping", direction: "asc" });
    expect(nextSort(null, "name")).toEqual({ column: "name", direction: "asc" });
    expect(nextSort(null, "region")).toEqual({ column: "region", direction: "asc" });
  });

  it("reverses the column already sorted", () => {
    const current: TableSort = { column: "players", direction: "desc" };
    expect(nextSort(current, "players")).toEqual({ column: "players", direction: "asc" });
  });

  it("does not carry a direction across to a different column", () => {
    // Otherwise clicking players (high–low) then name would sort names Z–A.
    const current: TableSort = { column: "players", direction: "desc" };
    expect(nextSort(current, "name")).toEqual({ column: "name", direction: "asc" });
  });
});

describe("isDatasetWide", () => {
  it("is true only for the orderings the API itself serves", () => {
    expect(isDatasetWide({ column: "players", direction: "desc" })).toBe(true);
    expect(isDatasetWide({ column: "name", direction: "asc" })).toBe(true);
  });

  it("is false in the reverse direction, which the API has no parameter for", () => {
    expect(isDatasetWide({ column: "players", direction: "asc" })).toBe(false);
    expect(isDatasetWide({ column: "name", direction: "desc" })).toBe(false);
  });

  it("is false for every column the API cannot sort by", () => {
    for (const column of ["queue", "ping", "mods", "region"] as const) {
      expect(isDatasetWide({ column, direction: "desc" })).toBe(false);
      expect(isDatasetWide({ column, direction: "asc" })).toBe(false);
      expect(API_SORTABLE[column]).toBeUndefined();
    }
  });
});

describe("sortRows", () => {
  it("returns the rows untouched when nothing is sorted", () => {
    const rows = [server({ id: "a" }), server({ id: "b" })];
    // Identity, not just equality: the browse view re-renders on a new array.
    expect(sortRows(rows, null, noPings)).toBe(rows);
  });

  it("does not mutate the array it was given", () => {
    const rows = [server({ id: "a", players: 1 }), server({ id: "b", players: 9 })];
    sortRows(rows, { column: "players", direction: "desc" }, noPings);
    expect(ids(rows)).toEqual(["a", "b"]);
  });

  it("orders by players in both directions", () => {
    const rows = [
      server({ id: "mid", players: 40 }),
      server({ id: "high", players: 120 }),
      server({ id: "low", players: 3 }),
    ];
    expect(ids(sortRows(rows, { column: "players", direction: "desc" }, noPings))).toEqual([
      "high",
      "mid",
      "low",
    ]);
    expect(ids(sortRows(rows, { column: "players", direction: "asc" }, noPings))).toEqual([
      "low",
      "mid",
      "high",
    ]);
  });

  it("orders by name on the tidied text, as shown", () => {
    const rows = [
      server({ id: "b", name: "  [EU] Bravo  " }),
      server({ id: "a", name: "[EU]  Alpha" }),
    ];
    expect(ids(sortRows(rows, { column: "name", direction: "asc" }, noPings))).toEqual([
      "a",
      "b",
    ]);
  });

  it("orders by queue and mod count", () => {
    const rows = [
      server({ id: "few", queue: 2, modCount: 5 }),
      server({ id: "many", queue: 40, modCount: 190 }),
    ];
    expect(ids(sortRows(rows, { column: "queue", direction: "desc" }, noPings))).toEqual([
      "many",
      "few",
    ]);
    expect(ids(sortRows(rows, { column: "mods", direction: "desc" }, noPings))).toEqual([
      "many",
      "few",
    ]);
  });

  it("orders regions by the label on screen, not the raw id", () => {
    // "new_york" renders as "New York", which belongs under N — before Sydney.
    const rows = [
      server({ id: "s", pingSiteId: "sydney" }),
      server({ id: "n", pingSiteId: "new_york" }),
      server({ id: "f", pingSiteId: "frankfurt" }),
    ];
    expect(ids(sortRows(rows, { column: "region", direction: "asc" }, pings))).toEqual([
      "f",
      "n",
      "s",
    ]);
  });

  it("orders by measured latency", () => {
    const rows = [
      server({ id: "far", pingSiteId: "sydney" }),
      server({ id: "near", pingSiteId: "frankfurt" }),
      server({ id: "mid", pingSiteId: "new_york" }),
    ];
    expect(ids(sortRows(rows, { column: "ping", direction: "asc" }, pings))).toEqual([
      "near",
      "mid",
      "far",
    ]);
  });

  it("keeps unmeasured regions last in both directions", () => {
    // The case that must not regress: an unknown latency shown as the fastest
    // thing on the page would be actively misleading.
    const rows = [
      server({ id: "unknown", pingSiteId: null }),
      server({ id: "far", pingSiteId: "sydney" }),
      server({ id: "near", pingSiteId: "frankfurt" }),
    ];
    expect(ids(sortRows(rows, { column: "ping", direction: "asc" }, pings))).toEqual([
      "near",
      "far",
      "unknown",
    ]);
    expect(ids(sortRows(rows, { column: "ping", direction: "desc" }, pings))).toEqual([
      "far",
      "near",
      "unknown",
    ]);
  });

  it("settles ties the same way every time", () => {
    // Every vanilla server has zero mods; without a tie-break they would
    // reshuffle on each refresh, because the source array is rebuilt per request.
    const rows = [
      server({ id: "quiet", name: "Zulu", players: 2 }),
      server({ id: "busy", name: "Alpha", players: 60 }),
      server({ id: "alsoBusy", name: "Bravo", players: 60 }),
    ];
    const sort: TableSort = { column: "mods", direction: "desc" };
    const once = ids(sortRows(rows, sort, noPings));
    const twice = ids(sortRows([...rows].reverse(), sort, noPings));
    expect(once).toEqual(["busy", "alsoBusy", "quiet"]);
    expect(twice).toEqual(once);
  });
});
