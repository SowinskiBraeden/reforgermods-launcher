/**
 * Compact filter row.
 *
 * Only filters `/v2/servers` actually supports appear here, plus the one
 * client-side control (minimum players), which is labelled as page-local so it
 * does not read as a server-side filter.
 */

import { useEffect, useRef } from "preact/hooks";
import {
  clearFilters,
  clientSort,
  pingSites,
  query,
  searchInput,
  setQuery,
  setSearch,
} from "../lib/state";
import { humaniseRegion } from "../lib/format";
import type { ClientPlatform, ServerSort } from "../lib/types";

/** Cycles a tri-state filter: unset -> true -> false -> unset. */
function nextTriState(value: boolean | null): boolean | null {
  if (value === null) return true;
  if (value) return false;
  return null;
}

function triLabel(value: boolean | null): "on" | "off" | "unset" {
  return value === null ? "unset" : value ? "on" : "off";
}

interface ChipProps {
  label: string;
  value: boolean | null;
  onChange: (next: boolean | null) => void;
  titles: [unset: string, on: string, off: string];
}

function Chip({ label, value, onChange, titles }: ChipProps) {
  const state = triLabel(value);
  const title = state === "unset" ? titles[0] : state === "on" ? titles[1] : titles[2];
  return (
    <button
      class="chip"
      data-state={state}
      title={title}
      aria-pressed={value !== null}
      onClick={() => onChange(nextTriState(value))}
    >
      {label}
    </button>
  );
}

export function FilterBar() {
  const q = query.value;
  const searchRef = useRef<HTMLInputElement>(null);

  // Ctrl+K focuses search, Escape clears it. Registered once for the window so
  // the shortcut works regardless of what currently has focus.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      } else if (event.key === "Escape" && document.activeElement === searchRef.current) {
        searchRef.current?.blur();
        if (searchInput.value) setSearch("");
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const sites = pingSites.value;

  return (
    <div class="toolbar">
      <div class="search">
        <input
          ref={searchRef}
          type="text"
          spellcheck={false}
          autocomplete="off"
          placeholder="Search servers"
          aria-label="Search servers"
          value={searchInput.value}
          onInput={(e) => setSearch((e.target as HTMLInputElement).value)}
        />
        <span class="search__hint">Ctrl F</span>
      </div>

      <select
        class="control"
        aria-label="Sort order"
        value={clientSort.value?.column === "ping" ? "ping" : q.sort}
        onChange={(e) => {
          const value = (e.target as HTMLSelectElement).value;
          if (value === "ping") {
            // The API cannot sort by latency — it does not know the user's — so
            // fetch the busiest servers and reorder the page locally.
            clientSort.value = { column: "ping", direction: "asc" };
            setQuery({ sort: "players" });
          } else {
            // A dataset-wide sort supersedes any page-local one, including a
            // header click that reversed a column.
            clientSort.value = null;
            setQuery({ sort: value as ServerSort });
          }
        }}
      >
        <option value="players">Most players</option>
        <option value="ping">Lowest ping</option>
        <option value="name">Name</option>
        <option value="newest">Newest</option>
        <option value="lastSeen">Last seen</option>
      </select>

      <select
        class="control"
        aria-label="Ping region"
        value={q.region ?? ""}
        onChange={(e) => {
          const value = (e.target as HTMLSelectElement).value;
          setQuery({ region: value === "" ? null : value });
        }}
      >
        <option value="">All regions</option>
        {sites.map((site) => (
          <option key={site.id} value={site.id}>
            {site.label || humaniseRegion(site.id)}
          </option>
        ))}
      </select>

      <select
        class="control"
        aria-label="Client platform"
        value={q.platform ?? ""}
        onChange={(e) => {
          const value = (e.target as HTMLSelectElement).value;
          setQuery({ platform: value === "" ? null : (value as ClientPlatform) });
        }}
      >
        <option value="">Any platform</option>
        <option value="pc">PC</option>
        <option value="xbox">Xbox</option>
        <option value="psn">PlayStation</option>
      </select>


      <Chip
        label="Official"
        value={q.official}
        onChange={(official) => setQuery({ official })}
        titles={["Any", "Official only", "Community only"]}
      />
      <Chip
        label="Modded"
        value={q.hasMods}
        onChange={(hasMods) => setQuery({ hasMods })}
        titles={["Any", "Modded only", "Vanilla only"]}
      />
      <Chip
        label="BattlEye"
        value={q.battleye}
        onChange={(battleye) => setQuery({ battleye })}
        titles={["Any", "BattlEye on", "BattlEye off"]}
      />
      <Chip
        label="Password"
        value={q.locked}
        onChange={(locked) => setQuery({ locked })}
        titles={["Any", "Locked only", "Open only"]}
      />

      <button
        class="chip"
        data-state={q.includeOffline ? "on" : "unset"}
        aria-pressed={q.includeOffline}
        title="Include servers not currently listed"
        onClick={() => setQuery({ includeOffline: !q.includeOffline })}
      >
        Offline
      </button>

      <span class="control-label">Min players</span>
      <input
        class="control"
        type="number"
        min={0}
        max={256}
        step={1}
        aria-label="Minimum players"
        title="Filters the loaded page"
        value={q.minPlayers}
        onInput={(e) => {
          const raw = Number((e.target as HTMLInputElement).value);
          setQuery({ minPlayers: Number.isFinite(raw) ? Math.max(0, Math.trunc(raw)) : 0 });
        }}
      />

      <div class="toolbar__spacer" />
      {hasActiveFilters(q) && (
        <button class="linkish" onClick={clearFilters}>
          Clear filters
        </button>
      )}
    </div>
  );
}

function hasActiveFilters(q: typeof query.value): boolean {
  return (
    q.search.trim() !== "" ||
    q.region !== null ||
    q.platform !== null ||
    q.official !== null ||
    q.hasMods !== null ||
    q.battleye !== null ||
    q.locked !== null ||
    q.includeOffline ||
    q.minPlayers > 0
  );
}
