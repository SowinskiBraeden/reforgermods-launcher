/**
 * Favorites and recent servers.
 *
 * Both hold ids locally and render the same table as the browser, resolved to
 * live rows, so a favorite shows its current population, ping and flags rather
 * than a remembered name. The stored name is only a fallback for the empty and
 * error states.
 */

import { ServerTable } from "../components/ServerTable";
import {
  favorites,
  loadSavedList,
  nowMs,
  recents,
  savedListLoading,
  setView,
  visibleFavorites,
  visibleRecents,
} from "../lib/state";
import { formatTimestamp } from "../lib/format";

interface Props {
  title: string;
  lede: string;
  /** Rows resolved from the stored ids. */
  rows: typeof visibleFavorites.value;
  /** How many ids are stored, which may exceed the rows that still resolve. */
  storedCount: number;
  emptyTitle: string;
  emptyHint: string;
  onReload: () => void;
}

function SavedList(props: Props) {
  const loading = savedListLoading.value;
  const { rows, storedCount } = props;

  if (storedCount === 0) {
    return (
      <div class="browser">
        <div class="state">
          <div class="state__title">{props.emptyTitle}</div>
          <div class="state__hint">{props.emptyHint}</div>
          <button class="btn" onClick={() => setView("browse")}>
            Open the server browser
          </button>
        </div>
      </div>
    );
  }

  // Servers drop out of the index; say so rather than quietly showing fewer.
  const missing = storedCount - rows.length;

  return (
    <div class="browser">
      <div class="toolbar">
        <span class="control-label">{props.title}</span>
        <span style={{ fontSize: "11px", color: "var(--text-dim)" }}>{props.lede}</span>
        <div class="toolbar__spacer" />
        <button class="linkish" disabled={loading} onClick={props.onReload}>
          {loading ? "Refreshing…" : "Refresh"}
        </button>
      </div>
      {loading ? <div class="loadbar" /> : <div style={{ height: "1px" }} />}

      {missing > 0 && rows.length > 0 && (
        <div class="banner">
          {missing} of {storedCount} are not in the current server index.
        </div>
      )}

      {rows.length === 0 && !loading ? (
        <div class="state">
          <div class="state__title">None of these are currently listed</div>
          <div class="state__hint">
            The servers are still saved. They will reappear here when they return to
            the index.
          </div>
        </div>
      ) : (
        <ServerTable servers={rows} paginated={false} />
      )}

      <div class="footbar">
        <span>
          {rows.length} listed
          {storedCount !== rows.length && ` of ${storedCount} saved`}
        </span>
      </div>
    </div>
  );
}

export function FavoritesView() {
  return (
    <SavedList
      title="Favorites"
      lede="Stored locally, shown with live population."
      rows={visibleFavorites.value}
      storedCount={favorites.value.length}
      emptyTitle="No favorites yet"
      emptyHint="Star a server in the browser and it will be kept here across restarts."
      onReload={() => void loadSavedList("favorites")}
    />
  );
}

export function RecentView() {
  const items = recents.value;
  const newest = items[0];
  // Subscribe to the clock so the "last opened" line stays live.
  const now = nowMs.value;

  return (
    <SavedList
      title="Recent"
      lede={
        newest
          ? `Most recent ${formatTimestamp(newest.lastSeenAt, now)}.`
          : "Servers you open appear here."
      }
      rows={visibleRecents.value}
      storedCount={items.length}
      emptyTitle="No recent servers"
      emptyHint="Servers you open appear here, most recent first."
      onReload={() => void loadSavedList("recent")}
    />
  );
}
