/** The server browser: filters, table, and the pager/status footer. */

import { FilterBar } from "../components/FilterBar";
import { ServerTable } from "../components/ServerTable";
import {
  clearFilters,
  filteredOut,
  listError,
  listInitialising,
  listLoading,
  loadServers,
  query,
  setQuery,
  sortIsPageLocal,
  activeSort,
  totalPages,
  totalServers,
  visibleServers,
} from "../lib/state";
import { COLUMN_LABEL } from "../lib/sort";

export function BrowseView() {
  const rows = visibleServers.value;
  const error = listError.value;
  const loading = listLoading.value;
  const initialising = listInitialising.value;
  const q = query.value;

  return (
    <div class="browser">
      <FilterBar />
      {loading ? <div class="loadbar" /> : <div style={{ height: "1px" }} />}

      {error && (
        <div class="banner banner--error">
          <span>{errorHeadline(error.kind)}</span>
          <span style={{ color: "var(--text-dim)" }}>{error.message}</span>
          <div class="toolbar__spacer" />
          <button class="linkish" onClick={() => void loadServers()}>
            Retry
          </button>
        </div>
      )}

      {filteredOut.value > 0 && (
        <div class="banner">
          {filteredOut.value} server{filteredOut.value === 1 ? "" : "s"} on this page hidden
          by the minimum-player filter.
        </div>
      )}

      {initialising ? (
        <div class="state">
          <div class="state__title">Loading servers…</div>
          <div class="state__hint">Fetching the current snapshot from reforgermods.net.</div>
        </div>
      ) : rows.length === 0 && !error ? (
        <div class="state">
          <div class="state__title">No servers match these filters</div>
          <div class="state__hint">
            Try a broader search, or clear the filters to see the full list.
          </div>
          <button class="btn" onClick={clearFilters}>
            Clear filters
          </button>
        </div>
      ) : rows.length === 0 && error ? (
        <div class="state">
          <div class="state__title">Nothing to show</div>
          <div class="state__hint">
            The launcher could not reach reforgermods.net, and has no cached page to fall
            back to.
          </div>
        </div>
      ) : (
        <ServerTable servers={rows} />
      )}

      <div class="footbar">
        <span>
          {rows.length} shown
          {totalServers.value > 0 && ` of ${totalServers.value.toLocaleString()} matched`}
          {/* A sort the API cannot serve reorders these rows and no others.
              Saying so here is the same contract as the minimum-player filter
              being labelled page-local rather than presented as a search. */}
          {sortIsPageLocal.value && activeSort.value && (
            <span style={{ color: "var(--text-faint)" }}>
              {" "}
              · sorted by {COLUMN_LABEL[activeSort.value.column]} within this page
            </span>
          )}
        </span>
        <div class="footbar__spacer" />
        {totalPages.value > 1 && (
          <div class="pager">
            <button
              disabled={q.page <= 1 || loading}
              onClick={() => setQuery({ page: q.page - 1 })}
              aria-label="Previous page"
            >
              ‹
            </button>
            <span>
              {q.page} / {totalPages.value}
            </span>
            <button
              disabled={q.page >= totalPages.value || loading}
              onClick={() => setQuery({ page: q.page + 1 })}
              aria-label="Next page"
            >
              ›
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function errorHeadline(kind: string): string {
  switch (kind) {
    case "offline":
      return "Offline";
    case "timeout":
      return "Timed out";
    case "rate_limited":
      return "Rate limited";
    case "decode":
      return "Unexpected response";
    default:
      return "Request failed";
  }
}
