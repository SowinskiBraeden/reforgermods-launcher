/**
 * Selected-server detail pane.
 *
 * Shows only values the API actually returns, or that were measured locally.
 * There is deliberately no uptime figure, and mod readiness is reported without
 * any action attached: the launcher cannot install Workshop content, so it says
 * what the game will do instead. See `docs/mod-readiness.md`.
 */

import {
  detailError,
  detailLoading,
  favoriteSet,
  join,
  joinError,
  joining,
  ping,
  pingLoading,
  selectServer,
  selectedId,
  selectedServer,
  toggleFavorite,
} from "../lib/state";
import { HistoryChart } from "./HistoryChart";
import { PlatformBadges } from "./PlatformBadges";
import { StatusMarks } from "./StatusMarks";
import { ReadinessNote } from "./ReadinessNote";
import { openDiscordInvite, openServerPage } from "../lib/ipc";
import {
  discordInviteFrom,
  formatBytes,
  formatPlayers,
  humaniseRegion,
  safeImageUrl,
  tidyName,
} from "../lib/format";
import type { ServerDetail } from "../lib/types";

export function Inspector() {
  const id = selectedId.value;
  const server = selectedServer.value;
  const error = detailError.value;

  if (!id) {
    return (
      <aside class="inspector">
        <div class="state">
          <div class="state__title">No server selected</div>
          <div class="state__hint">
            Pick a server to see its scenario, population, mods and join it directly
            through Steam.
          </div>
        </div>
      </aside>
    );
  }

  if (error) {
    return (
      <aside class="inspector">
        <div class="inspector__head">
          <div class="inspector__title">Could not load server</div>
        </div>
        <div class="inspector__body">
          <p class="state__hint">{error.message}</p>
          <div class="btn-row">
            <button class="btn" onClick={() => void selectServer(id)}>
              Try again
            </button>
            <button class="btn" onClick={() => void selectServer(null)}>
              Close
            </button>
          </div>
        </div>
      </aside>
    );
  }

  if (!server) {
    return (
      <aside class="inspector">
        <div class="inspector__head">
          <div class="inspector__title">Loading…</div>
        </div>
      </aside>
    );
  }

  return <Detail server={server} isFavorite={favoriteSet.value.has(server.id)} />;
}

function Detail({ server, isFavorite }: { server: ServerDetail; isFavorite: boolean }) {
  const refreshing = detailLoading.value;
  const busy = joining.value;
  const failure = joinError.value;

  // About half of servers publish no imagery, and any URL the API sends is
  // untrusted, so this is null more often than not.
  const image = safeImageUrl(server.scenarioThumbnailUrl);
  // Server operators advertise their Discord in the server name; the same
  // detection the website uses surfaces it as a button.
  const discord = discordInviteFrom(server.name);

  return (
    <aside class="inspector">
      {image && (
        <div class="scenario">
          <img
            class="scenario__img"
            src={image}
            alt=""
            loading="lazy"
            decoding="async"
            referrerPolicy="no-referrer"
            /* A CDN hiccup should leave the panel tidy, not show a broken icon. */
            onError={(e) => {
              (e.currentTarget as HTMLImageElement).closest(".scenario")?.remove();
            }}
          />
        </div>
      )}
      <div class="inspector__head">
        <div class="inspector__title">{tidyName(server.name)}</div>
        <div class="inspector__sub">
          {server.scenarioName || "Unnamed scenario"}
          {server.gameVersion && ` — ${server.gameVersion}`}
        </div>
        <div class="tags" style={{ marginTop: "7px", flexWrap: "wrap" }}>
          <span class={`tag ${server.online ? "tag--accent" : "tag--warn"}`}>
            {server.online ? "ONLINE" : "NOT LISTED"}
          </span>
          {/* The same marks as the table row, so the two views agree. */}
          <StatusMarks server={server} />
          <PlatformBadges platforms={server.platforms} />
        </div>
      </div>

      <div class="inspector__body">
        <div class="facts">
          <Fact k="Players" v={formatPlayers(server.players, server.maxPlayers)} />
          <Fact
            k="Queue"
            v={server.queueMax > 0 ? `${server.queue} / ${server.queueMax}` : "—"}
          />
          <Fact k="Region" v={humaniseRegion(server.pingSiteId ?? server.region)} />
          <PingFact />
          {/* The API reports the server's simulation FPS, not a client ping. */}
          <Fact k="Server FPS" v={server.fps > 0 ? String(server.fps) : "—"} />
          <Fact k="Mods" v={server.modCount > 0 ? String(server.modCount) : "None"} />
          {/* Published by the API; shown as text and never acted on. */}
          <Fact
            k="Address"
            v={server.host ? `${server.host}:${server.port}` : "Not published"}
          />
          <Fact k="Host" v={server.platform ?? "Unknown"} />
        </div>

        <HistoryChart />

        {server.mods.length > 0 && (
          <div class="section">
            <div class="section__h">
              <span>Required mods</span>
              <span>
                {server.mods.length} total
                {server.modSummary.knownSize > 0 &&
                  `, ${formatBytes(server.modSummary.knownSize)}`}
              </span>
            </div>
            <ReadinessNote />
            <div class="modlist">
              {/* Three columns rather than one concatenated string, so versions
                  and sizes line up and the list can be scanned down. */}
              {server.mods.map((mod) => (
                <div class="modlist__row" key={mod.id}>
                  <span class="modlist__name" title={mod.name || mod.id}>
                    {mod.name || mod.id}
                  </span>
                  <span class="modlist__version">{mod.version || "—"}</span>
                  <span class="modlist__size">
                    {mod.sizeKnown && mod.size > 0 ? formatBytes(mod.size) : ""}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      <div class="inspector__foot">
        {failure && (
          <p class="state__hint" style={{ color: "var(--bad)", marginTop: 0 }}>
            {failure.message}
          </p>
        )}
        <button
          class="btn btn--primary"
          disabled={busy || !server.online}
          title={server.online ? "Close Arma Reforger first if it is running" : "Not listed"}
          onClick={() => void join({ id: server.id, name: server.name })}
        >
          {busy ? "Handing off to Steam…" : "Join server"}
        </button>
        {discord && (
          <button
            class="btn btn--discord"
            title={`discord.gg/${discord.code}`}
            onClick={() => void openDiscordInvite(discord.code)}
          >
            Join Discord
          </button>
        )}
        <div class="btn-row">
          <button class="btn" onClick={() => void toggleFavorite(server.id)}>
            {isFavorite ? "★ Favorited" : "☆ Favorite"}
          </button>
          <button
            class="btn"
            title="Open on reforgermods.net"
            onClick={() => void openServerPage(server.id)}
          >
            Open page
          </button>
          <button
            class="btn"
            disabled={refreshing}
            onClick={() => void selectServer(server.id)}
          >
            {refreshing ? "…" : "Refresh"}
          </button>
        </div>
      </div>
    </aside>
  );
}

/**
 * Measured round-trip latency to the server, by ICMP.
 *
 * Shows the median of three probes. When the host does not answer — about a
 * third of servers filter ICMP — it says so rather than substituting a
 * region-level estimate, which would look like a per-server measurement.
 */
function PingFact() {
  const result = ping.value;
  const loading = pingLoading.value;

  if (loading && !result) return <Fact k="Ping" v="…" />;
  const summary = result?.summary;
  if (!summary) {
    return (
      <div class="fact" title={result?.message ?? "Not measured."}>
        <div class="fact__k">Ping</div>
        <div class="fact__v fact__v--muted">—</div>
      </div>
    );
  }
  const loss = summary.sent > 0 ? Math.round(((summary.sent - summary.received) / summary.sent) * 100) : 0;
  return (
    <div
      class="fact"
      title={`Best ${summary.bestMs} ms`}
    >
      <div class="fact__k">Ping{loss > 0 ? ` (${loss}% loss)` : ""}</div>
      <div class="fact__v">{summary.medianMs} ms</div>
    </div>
  );
}

function Fact({ k, v }: { k: string; v: string }) {
  return (
    <div class="fact">
      <div class="fact__k">{k}</div>
      <div class="fact__v" title={v}>
        {v}
      </div>
    </div>
  );
}
