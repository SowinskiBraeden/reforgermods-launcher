/**
 * Application state.
 *
 * Signals rather than a store framework: each component subscribes only to the
 * signals it reads, so the one-second clock behind "updated 18s ago" re-renders
 * the status pill and nothing else. The server table re-renders only when the
 * server array itself changes.
 */

import { computed, signal } from "@preact/signals";
import * as ipc from "./ipc";
import {
  API_SORTABLE,
  isDatasetWide,
  nextSort,
  sortRows,
  type SortColumn,
  type TableSort,
} from "./sort";
import {
  DEFAULT_QUERY,
  isCommandError,
  type CommandError,
  type FleetNow,
  type HistoryRange,
  type InstallIdentity,
  type PingResult,
  type ReadinessReport,
  type ServerHistory,
  type LaunchRecord,
  type LauncherStatus,
  type PingSite,
  type RecentServer,
  type ServerDetail,
  type ServerQuery,
  type ServerSummary,
  type Settings,
} from "./types";

export type View = "browse" | "favorites" | "recent" | "settings";

/** Debounce applied to the search field before a request is issued. */
export const SEARCH_DEBOUNCE_MS = 250;
/** How often the header re-reads API status. */
const STATUS_POLL_MS = 60_000;

// ---------------------------------------------------------------- navigation

export const view = signal<View>("browse");

// -------------------------------------------------------------- server list

export const query = signal<ServerQuery>({ ...DEFAULT_QUERY });
/** The raw search field, before debouncing into `query`. */
export const searchInput = signal("");

export const servers = signal<ServerSummary[]>([]);
export const listLoading = signal(false);
/** True only for the very first load, so refreshes do not blank the table. */
export const listInitialising = signal(true);
export const listError = signal<CommandError | null>(null);
export const totalServers = signal(0);
export const totalPages = signal(0);
export const filteredOut = signal(0);
/**
 * Latency per ping-site id, measured once and reused for every row.
 *
 * Region latency, not per-server: see `ping_regions` in the Rust command layer
 * for why a per-row measurement is not affordable.
 */
export const regionPings = signal<Record<string, PingResult>>({});

/**
 * Client-side ordering applied on top of the API's sort.
 *
 * Null means the rows are shown in the order the API returned them. Anything
 * else reorders only the rows already loaded — see `./sort` for why that
 * distinction is kept rather than smoothed over.
 */
export const clientSort = signal<TableSort | null>(null);

// ------------------------------------------------- favorites & recents lists

/** Live rows for the favorites view, resolved from stored ids. */
export const favoriteServers = signal<ServerSummary[]>([]);
/** Live rows for the recents view, resolved from stored ids. */
export const recentServers = signal<ServerSummary[]>([]);
export const savedListLoading = signal(false);

// ------------------------------------------------------------------ detail

export const selectedId = signal<string | null>(null);
export const selectedServer = signal<ServerDetail | null>(null);
export const detailLoading = signal(false);
export const detailError = signal<CommandError | null>(null);
/** Set while a join is in flight, so the button can show progress. */
export const joining = signal(false);
export const joinError = signal<CommandError | null>(null);

// ------------------------------------------------------- history & readiness

export const historyRange = signal<HistoryRange>("oneDay");
export const historySeries = signal<ServerHistory | null>(null);
export const historyLoading = signal(false);
/** Null while unknown; the report itself says whether a scan was possible. */
export const readiness = signal<ReadinessReport | null>(null);
/** Null while unmeasured; the result itself says whether there is a figure. */
export const ping = signal<PingResult | null>(null);
export const pingLoading = signal(false);

// ------------------------------------------------------------ local storage

export const settings = signal<Settings | null>(null);
/** True when started with RFM_DEBUG. Gates developer-facing detail. */
export const debugMode = signal(false);
/** Install identity. Populated only in debug mode. */
export const installIdentity = signal<InstallIdentity | null>(null);
export const favorites = signal<string[]>([]);
export const recents = signal<RecentServer[]>([]);
export const history = signal<LaunchRecord[]>([]);

export const favoriteSet = computed(() => new Set(favorites.value));

/** Median latency for a server's region, or null when unmeasured. */
export function pingFor(pingSiteId: string | null): number | null {
  if (!pingSiteId) return null;
  return regionPings.value[pingSiteId]?.summary?.medianMs ?? null;
}

/** The browse rows, after any client-side ordering. */
export const visibleServers = computed(() =>
  sortRows(servers.value, clientSort.value, pingFor),
);

/**
 * Favorites and recents, under the same ordering.
 *
 * These lists are resolved in full rather than paginated, so a sort here covers
 * everything the view can show and carries no page-local caveat.
 */
export const visibleFavorites = computed(() =>
  sortRows(favoriteServers.value, clientSort.value, pingFor),
);
export const visibleRecents = computed(() =>
  sortRows(recentServers.value, clientSort.value, pingFor),
);

/**
 * The ordering currently in effect, for the header indicators.
 *
 * With no client sort, this reports the API's own ordering so the column the
 * server sorted by is still marked — `newest` and `lastSeen` have no column and
 * mark nothing.
 */
export const activeSort = computed<TableSort | null>(() => {
  if (clientSort.value) return clientSort.value;
  switch (query.value.sort) {
    case "players":
      return { column: "players", direction: "desc" };
    case "name":
      return { column: "name", direction: "asc" };
    default:
      return null;
  }
});

/**
 * True when the active ordering reaches only the loaded page.
 *
 * A client sort that happens to match what the API already did is not
 * page-local: the rows are the dataset's top rows either way.
 */
export const sortIsPageLocal = computed(() => {
  const sort = clientSort.value;
  if (!sort) return false;
  const api = API_SORTABLE[sort.column];
  return !(api && api.direction === sort.direction && query.value.sort === api.sort);
});

/**
 * Applies a header click.
 *
 * `datasetWide` is false for the favorites and recents tables, where there is
 * no API query behind the rows and every sort is therefore local to the list
 * being shown — and complete, because the list is not paginated.
 */
export function sortByColumn(column: SortColumn, datasetWide = true): void {
  const next = nextSort(activeSort.value, column);
  if (datasetWide && isDatasetWide(next)) {
    // The API can order the whole dataset this way, so ask it to and drop the
    // local reordering entirely.
    clientSort.value = null;
    setQuery({ sort: API_SORTABLE[next.column]!.sort });
    return;
  }
  clientSort.value = next;
}

// ------------------------------------------------------------------ status

export const status = signal<LauncherStatus | null>(null);
export const pingSites = signal<PingSite[]>([]);
/** Fleet-wide population, shown in the title bar. */
export const fleet = signal<FleetNow | null>(null);
/** Ticks once a second purely so relative timestamps stay live. */
export const nowMs = signal(Date.now());

// ---------------------------------------------------------------- internals

/** Guards against a slow response overwriting a newer one. */
let listRequestSeq = 0;
let detailRequestSeq = 0;
let historyRequestSeq = 0;
let readinessRequestSeq = 0;
let pingRequestSeq = 0;
let savedListSeq = 0;
let searchTimer: ReturnType<typeof setTimeout> | undefined;

function toCommandError(err: unknown): CommandError {
  if (isCommandError(err)) return err;
  return { kind: "unknown", message: String(err) };
}

// ----------------------------------------------------------------- actions

/** Replaces the query, resetting to page 1 unless the page itself changed. */
export function setQuery(patch: Partial<ServerQuery>): void {
  const next = { ...query.value, ...patch };
  if (patch.page === undefined) next.page = 1;
  query.value = next;
  void loadServers();
}

/** Updates the search box and schedules a debounced reload. */
export function setSearch(value: string): void {
  searchInput.value = value;
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    if (searchInput.value !== query.value.search) {
      setQuery({ search: searchInput.value });
    }
  }, SEARCH_DEBOUNCE_MS);
}

/** Clears every filter, keeping sort and page size. */
export function clearFilters(): void {
  searchInput.value = "";
  clearTimeout(searchTimer);
  setQuery({
    search: "",
    region: null,
    platform: null,
    official: null,
    hasMods: null,
    battleye: null,
    locked: null,
    includeOffline: false,
    minPlayers: 0,
  });
}

export async function loadServers(): Promise<void> {
  const seq = ++listRequestSeq;
  listLoading.value = true;
  try {
    const result = await ipc.listServers(query.value);
    if (seq !== listRequestSeq) return; // Superseded by a newer request.
    servers.value = result.servers;
    totalServers.value = result.totalServers;
    totalPages.value = result.totalPages;
    filteredOut.value = result.filteredOut;
    listError.value = null;
    if (status.value) {
      // Every server response carries snapshot freshness, so the header stays
      // current without a status request of its own.
      status.value = { ...status.value, dataset: result.dataset };
    }
  } catch (err) {
    if (seq !== listRequestSeq) return;
    listError.value = toCommandError(err);
  } finally {
    if (seq === listRequestSeq) {
      listLoading.value = false;
      listInitialising.value = false;
    }
  }
}

export async function selectServer(id: string | null): Promise<void> {
  selectedId.value = id;
  joinError.value = null;
  if (!id) {
    selectedServer.value = null;
    historySeries.value = null;
    readiness.value = null;
    ping.value = null;
    return;
  }
  const seq = ++detailRequestSeq;
  detailLoading.value = true;
  detailError.value = null;
  // Show what the list already knows while the detail request is in flight.
  const known = servers.value.find((s) => s.id === id);
  if (known && selectedServer.value?.id !== id) {
    selectedServer.value = null;
  }
  try {
    const detail = await ipc.getServer(id);
    if (seq !== detailRequestSeq) return;
    selectedServer.value = detail;
    // Both are secondary to the panel and must not delay it, so they are not
    // awaited. Readiness reuses the detail response the client just cached, so
    // it costs a directory scan rather than a request.
    void loadHistory(id);
    void loadReadiness(id);
    void measurePing(id);
    // The backend recorded the visit; mirror it locally without a round trip.
    recents.value = [
      { id: detail.id, name: detail.name, lastSeenAt: Math.floor(Date.now() / 1000) },
      ...recents.value.filter((r) => r.id !== detail.id),
    ].slice(0, 30);
  } catch (err) {
    if (seq !== detailRequestSeq) return;
    detailError.value = toCommandError(err);
  } finally {
    if (seq === detailRequestSeq) detailLoading.value = false;
  }
}

/** Loads population history for `id` at the current range. */
export async function loadHistory(id: string): Promise<void> {
  const seq = ++historyRequestSeq;
  historyLoading.value = true;
  try {
    const series = await ipc.getServerHistory(id, historyRange.value);
    if (seq !== historyRequestSeq || selectedId.value !== id) return;
    historySeries.value = series;
  } catch {
    if (seq !== historyRequestSeq) return;
    // The chart is supplementary; a failure renders as "no history", not an error.
    historySeries.value = null;
  } finally {
    if (seq === historyRequestSeq) historyLoading.value = false;
  }
}

/** Switches the history range and reloads for the selected server. */
export function setHistoryRange(range: HistoryRange): void {
  if (historyRange.value === range) return;
  historyRange.value = range;
  historySeries.value = null;
  const id = selectedId.value;
  if (id) void loadHistory(id);
}

/**
 * Loads the mod readiness report for `id`.
 *
 * Reporting only: nothing in the launcher acts on the result.
 */
export async function loadReadiness(id: string): Promise<void> {
  const seq = ++readinessRequestSeq;
  try {
    const report = await ipc.getModReadiness(id);
    if (seq !== readinessRequestSeq || selectedId.value !== id) return;
    readiness.value = report;
  } catch {
    if (seq !== readinessRequestSeq) return;
    // Leaving this null renders nothing, which is correct: an unknown state
    // must never be shown as a reassuring one.
    readiness.value = null;
  }
}

/**
 * Measures latency to the selected server.
 *
 * Only ever for the selected server: measuring a whole page would emit a
 * hundred ICMP probes on every refresh, which is both rude and indistinguishable
 * from a scan.
 */
export async function measurePing(id: string): Promise<void> {
  const seq = ++pingRequestSeq;
  pingLoading.value = true;
  ping.value = null;
  try {
    const result = await ipc.pingServer(id);
    if (seq !== pingRequestSeq || selectedId.value !== id) return;
    ping.value = result;
  } catch {
    if (seq !== pingRequestSeq) return;
    ping.value = null;
  } finally {
    if (seq === pingRequestSeq) pingLoading.value = false;
  }
}

/**
 * Resolves the favorites or recents list into live server rows.
 *
 * Those views hold ids, so a table needs one detail request per row. Both lists
 * are small and the client caches responses, so revisiting a tab is usually
 * free; ids that no longer resolve are dropped by the backend.
 */
export async function loadSavedList(which: "favorites" | "recent"): Promise<void> {
  const ids = which === "favorites" ? favorites.value : recents.value.map((r) => r.id);
  const target = which === "favorites" ? favoriteServers : recentServers;
  if (ids.length === 0) {
    target.value = [];
    return;
  }
  const seq = ++savedListSeq;
  savedListLoading.value = true;
  try {
    const resolved = await ipc.getServers(ids);
    if (seq !== savedListSeq) return;
    target.value = resolved;
  } catch {
    if (seq !== savedListSeq) return;
    // Leave whatever was shown; the stored list is still intact.
  } finally {
    if (seq === savedListSeq) savedListLoading.value = false;
  }
}

/** Switches view, resolving the saved lists on entry. */
export function setView(next: View): void {
  view.value = next;
  if (next === "favorites" || next === "recent") {
    void loadSavedList(next);
  }
}

export async function toggleFavorite(id: string): Promise<void> {
  try {
    const isFavorite = await ipc.toggleFavorite(id);
    favorites.value = isFavorite
      ? [...favorites.value, id]
      : favorites.value.filter((f) => f !== id);
    if (!isFavorite && view.value === "favorites") {
      favoriteServers.value = favoriteServers.value.filter((s) => s.id !== id);
    }
  } catch {
    // A failed toggle leaves the previous state visible, which is correct:
    // nothing was persisted.
  }
}

export async function join(server: { id: string; name: string }): Promise<void> {
  joining.value = true;
  joinError.value = null;
  try {
    await ipc.joinServer(server.id, server.name);
    history.value = [
      {
        serverId: server.id,
        serverName: server.name,
        launchedAt: Math.floor(Date.now() / 1000),
        succeeded: true,
      },
      ...history.value,
    ].slice(0, 200);
  } catch (err) {
    joinError.value = toCommandError(err);
  } finally {
    joining.value = false;
  }
}

export async function saveSettings(next: Settings): Promise<void> {
  const stored = await ipc.updateSettings(next);
  settings.value = stored;
  if (stored.perPage !== query.value.perPage) {
    setQuery({ perPage: stored.perPage });
  }
}


/** Refreshes the fleet-wide population shown in the title bar. */
export async function refreshFleet(): Promise<void> {
  try {
    fleet.value = await ipc.getFleetNow();
  } catch {
    // Decorative; the count simply stays as it was.
  }
}

export async function refreshStatus(): Promise<void> {
  try {
    const next = await ipc.getStatus();
    // Prefer freshness already seen on a server response over a status probe
    // that may not carry one.
    status.value = { ...next, dataset: next.dataset ?? status.value?.dataset ?? null };
  } catch {
    status.value = {
      apiReachable: false,
      dataset: status.value?.dataset ?? null,
      reportedStatus: null,
      note: null,
    };
  }
}

export async function clearLocalHistory(): Promise<void> {
  await ipc.clearHistory();
  recents.value = [];
  history.value = [];
}

let started = false;

/** Loads persisted state, then the first page of servers. */
export async function start(): Promise<void> {
  if (started) return;
  started = true;

  try {
    const local = await ipc.getLocalState();
    settings.value = local.settings;
    favorites.value = local.favorites;
    recents.value = local.recents;
    history.value = local.history;
    query.value = { ...query.value, perPage: local.settings.perPage };
    debugMode.value = await ipc.isDebugMode();
    if (debugMode.value) {
      installIdentity.value = await ipc.getInstallIdentity();
    }
  } catch {
    // Defaults are already in place; a storage failure must not block startup.
  }

  // The first server page is what the user is waiting for, so it goes first and
  // the decorative requests follow.
  await loadServers();

  void refreshStatus();
  void refreshFleet();
  // Eight ICMP probes, reused for every row in every page from here on.
  void ipc
    .pingRegions()
    .then((results) => {
      regionPings.value = results;
    })
    .catch(() => {
      // The ping column simply stays empty; it is supplementary.
    });
  void ipc
    .listPingSites()
    .then((sites) => {
      pingSites.value = sites;
    })
    .catch(() => {
      // Region labels fall back to humanised ids.
    });

  setInterval(() => {
    nowMs.value = Date.now();
  }, 1000);
  setInterval(() => {
    void refreshStatus();
    void refreshFleet();
  }, STATUS_POLL_MS);

  startAutoRefresh();
}

/**
 * Optional periodic reload of the server list.
 *
 * Off by default and floored at 15s by the settings store, and it skips while
 * the window is hidden so a backgrounded launcher makes no requests.
 */
function startAutoRefresh(): void {
  let lastRefreshMs = Date.now();
  // A single slow timer that checks the configured interval, rather than one
  // timer per settings change.
  setInterval(() => {
    const s = settings.value;
    if (!s?.autoRefresh) return;
    if (document.hidden) return; // A backgrounded window makes no requests.
    if (view.value !== "browse") return;
    if (Date.now() - lastRefreshMs < s.refreshIntervalSecs * 1000) return;
    lastRefreshMs = Date.now();
    void loadServers();
  }, 5_000);
}
