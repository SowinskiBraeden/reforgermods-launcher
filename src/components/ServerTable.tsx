/**
 * The server table.
 *
 * Windowed: only the rows intersecting the viewport are in the DOM, so a
 * 500-row page costs the same as a 20-row one. Row height is fixed in CSS
 * (`--row-h`) and mirrored here, which is what makes the arithmetic cheap
 * enough to run on every scroll frame.
 */

import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import {
  activeSort,
  favoriteSet,
  pingFor,
  selectServer,
  selectedId,
  sortByColumn,
  sortIsPageLocal,
  toggleFavorite,
} from "../lib/state";
import type { SortColumn } from "../lib/sort";
import { formatPlayers, humaniseRegion, tidyName } from "../lib/format";
import { PlatformBadges } from "./PlatformBadges";
import { FavouriteStar, StatusMarks } from "./StatusMarks";
import type { ServerSummary } from "../lib/types";

const ROW_HEIGHT = 30;
/** Rows rendered beyond the viewport, so fast scrolling does not show gaps. */
const OVERSCAN = 8;

interface Props {
  servers: ServerSummary[];
  /**
   * True when these rows are one page of a larger, API-ordered result.
   *
   * It decides both halves of the sorting behaviour: whether a header click can
   * ask the API to reorder the whole dataset, and whether an ordering it cannot
   * serve has to be marked as reaching only the loaded page. The favorites and
   * recents tables pass false — those lists are resolved in full, so sorting
   * them is always complete and never needs the caveat.
   */
  paginated?: boolean;
}

export function ServerTable({ servers, paginated = true }: Props) {
  const scrollerRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(600);

  useEffect(() => {
    const el = scrollerRef.current;
    if (!el) return;
    const measure = () => setViewportHeight(el.clientHeight);
    measure();

    // ResizeObserver catches layout changes the window never sees, such as the
    // inspector collapsing at narrow widths. Fall back to window resize where
    // it is unavailable, so the list still sizes correctly.
    if (typeof ResizeObserver === "function") {
      const observer = new ResizeObserver(measure);
      observer.observe(el);
      return () => observer.disconnect();
    }
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, []);

  const onScroll = useCallback((event: Event) => {
    setScrollTop((event.currentTarget as HTMLDivElement).scrollTop);
  }, []);

  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  const visibleCount = Math.ceil(viewportHeight / ROW_HEIGHT) + OVERSCAN * 2;
  const last = Math.min(servers.length, first + visibleCount);
  const visibleRows = servers.slice(first, last);

  const selected = selectedId.value;
  const favorites = favoriteSet.value;

  /** Moves the selection by `delta` rows and keeps it in view. */
  const move = useCallback(
    (delta: number) => {
      if (servers.length === 0) return;
      const current = servers.findIndex((s) => s.id === selectedId.value);
      const nextIndex = Math.min(
        servers.length - 1,
        Math.max(0, current === -1 ? 0 : current + delta),
      );
      const next = servers[nextIndex];
      if (!next) return;
      void selectServer(next.id);
      const el = scrollerRef.current;
      if (!el) return;
      const top = nextIndex * ROW_HEIGHT;
      if (top < el.scrollTop) el.scrollTop = top;
      else if (top + ROW_HEIGHT > el.scrollTop + el.clientHeight) {
        el.scrollTop = top + ROW_HEIGHT - el.clientHeight;
      }
    },
    [servers],
  );

  const onKeyDown = useCallback(
    (event: KeyboardEvent) => {
      switch (event.key) {
        case "ArrowDown":
          event.preventDefault();
          move(1);
          break;
        case "ArrowUp":
          event.preventDefault();
          move(-1);
          break;
        case "PageDown":
          event.preventDefault();
          move(Math.floor(viewportHeight / ROW_HEIGHT));
          break;
        case "PageUp":
          event.preventDefault();
          move(-Math.floor(viewportHeight / ROW_HEIGHT));
          break;
        case "Home":
          event.preventDefault();
          move(-servers.length);
          break;
        case "End":
          event.preventDefault();
          move(servers.length);
          break;
      }
    },
    [move, viewportHeight, servers.length],
  );

  return (
    <div
      class="table"
      ref={scrollerRef}
      onScroll={onScroll}
      onKeyDown={onKeyDown}
      tabIndex={0}
      role="listbox"
      aria-label="Servers"
      aria-activedescendant={selected ? `row-${selected}` : undefined}
    >
      {/* Inside the scroller, so the header and the rows share one width. With
          the header outside, a visible scrollbar narrowed the rows and every
          column drifted left of its heading. */}
      <div class="thead" role="presentation">
        <SortHeader column="name" label="Server" paginated={paginated} />
        <div />
        <SortHeader column="players" label="Players" align="num" paginated={paginated} />
        <SortHeader column="queue" label="Queue" align="num" paginated={paginated} />
        <SortHeader column="ping" label="Ping" align="mid" paginated={paginated} />
        <SortHeader column="region" label="Region" paginated={paginated} />
        <SortHeader column="mods" label="Mods" align="num" paginated={paginated} />
        {/* Not sortable: a server carries a set of platforms, not a value to
            order by. */}
        <div>Platform</div>
      </div>
      <div class="rows" style={{ height: `${servers.length * ROW_HEIGHT}px` }}>
        {visibleRows.map((server, i) => (
          <Row
            key={server.id}
            server={server}
            top={(first + i) * ROW_HEIGHT}
            isSelected={server.id === selected}
            isFavorite={favorites.has(server.id)}
          />
        ))}
      </div>
    </div>
  );
}

interface SortHeaderProps {
  column: SortColumn;
  label: string;
  /** Matches the column's cell alignment, so heading and values line up. */
  align?: "num" | "mid";
  paginated: boolean;
}

/**
 * A clickable column heading.
 *
 * The arrow is filled when the ordering covers every matching server and hollow
 * when it only reorders the rows already loaded — a distinction the API forces,
 * since it sorts by `players` and `name` alone and takes no direction. Shape
 * rather than colour alone carries it, and the tooltip says it in words.
 */
function SortHeader({ column, label, align, paginated }: SortHeaderProps) {
  const active = activeSort.value;
  const isActive = active?.column === column;
  const pageLocal = isActive && paginated && sortIsPageLocal.value;
  const ascending = active?.direction === "asc";

  let title: string;
  if (!isActive) {
    title = `Sort by ${label.toLowerCase()}`;
  } else if (pageLocal) {
    title = `Sorted by ${label.toLowerCase()} — reorders this page only`;
  } else if (paginated) {
    title = `Sorted by ${label.toLowerCase()} across all matching servers`;
  } else {
    title = `Sorted by ${label.toLowerCase()}`;
  }

  return (
    <button
      type="button"
      class={`th-sort${align ? ` ${align}` : ""}`}
      data-active={isActive}
      title={title}
      aria-label={title}
      onClick={() => sortByColumn(column, paginated)}
    >
      <span class="th-sort__label">{label}</span>
      {isActive && (
        <span class="th-sort__arrow" data-scope={pageLocal ? "page" : "all"} aria-hidden="true">
          {pageLocal ? (ascending ? "\u25b3" : "\u25bd") : ascending ? "\u25b2" : "\u25bc"}
        </span>
      )}
    </button>
  );
}

interface RowProps {
  server: ServerSummary;
  top: number;
  isSelected: boolean;
  isFavorite: boolean;
}

/**
 * Latency to the server's region.
 *
 * Region, not per-server: the list endpoint publishes no address, so eight
 * probes stand in for a whole page. The title says so, and the inspector shows
 * the exact per-server figure.
 */
function PingCell({ pingSiteId }: { pingSiteId: string | null }) {
  const ms = pingFor(pingSiteId);
  if (ms === null) return <div class="cell mid dim">—</div>;
  const band = ms < 60 ? "good" : ms < 140 ? "ok" : "far";
  return (
    <div class="cell mid ping" data-band={band} title="Region latency">
      {ms}
    </div>
  );
}

function Row({ server, top, isSelected, isFavorite }: RowProps) {
  return (
    <div
      id={`row-${server.id}`}
      class={`row${server.online ? "" : " row--offline"}`}
      style={{ top: `${top}px` }}
      role="option"
      aria-selected={isSelected}
      onClick={() => void selectServer(server.id)}
    >
      <div class="name">
        <FavouriteStar
          isFavorite={isFavorite}
          onToggle={() => void toggleFavorite(server.id)}
        />
        {/* Server names are untrusted text and are rendered as text content only. */}
        <span class="name__text" title={tidyName(server.name)}>
          {tidyName(server.name)}
        </span>
      </div>

      {/* Own column, so the marks line up instead of trailing each name. */}
      <StatusMarks server={server} />

      <div class="cell num" data-full={server.players >= server.maxPlayers}>
        {formatPlayers(server.players, server.maxPlayers)}
      </div>

      <div class="cell num dim">
        {server.queue > 0 ? `+${server.queue}` : "—"}
      </div>

      <PingCell pingSiteId={server.pingSiteId} />

      <div class="cell dim" title={server.pingSiteId ?? undefined}>
        {humaniseRegion(server.pingSiteId ?? server.region)}
      </div>

      <div class="cell num dim">{server.modCount > 0 ? server.modCount : "—"}</div>

      <PlatformBadges platforms={server.platforms} />
    </div>
  );
}
