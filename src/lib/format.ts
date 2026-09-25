/**
 * Pure display helpers.
 *
 * Kept free of DOM and Tauri imports so the formatting rules the UI depends on
 * can be unit-tested directly.
 */

import type { ReadinessReport } from "./types";

/**
 * Formats a byte count for a compact desktop layout.
 *
 * `digits` overrides the default precision. Table cells want one decimal so
 * columns stay narrow; a download figure the user is about to commit to wants
 * two, because 0.01 GiB is still 10 MB.
 */
export function formatBytes(bytes: number, digits?: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  // Whole bytes read oddly with a decimal, whatever the caller asked for.
  const decimals = unit === 0 ? 0 : (digits ?? (value >= 100 ? 0 : 1));
  return `${value.toFixed(decimals)} ${units[unit]}`;
}

/**
 * Formats an age in seconds as a short relative label, e.g. `18s ago`.
 *
 * Returns `just now` under a second so the header never flashes `0s ago`.
 */
export function formatAge(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "unknown";
  const s = Math.floor(seconds);
  if (s < 1) return "just now";
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

/** Formats a unix-seconds timestamp as an age relative to `now`. */
export function formatTimestamp(unixSeconds: number, now = Date.now()): string {
  return formatAge(now / 1000 - unixSeconds);
}

/**
 * Turns a ping-site id into a readable label when the API has not supplied one,
 * e.g. `new_york` -> `New York`.
 */
export function humaniseRegion(id: string | null | undefined): string {
  if (!id) return "—";
  return id
    .split(/[_-]/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

/** `12 / 128`, with the queue appended when one exists. */
export function formatPlayers(players: number, max: number): string {
  return `${players} / ${max}`;
}



/**
 * Collapses whitespace in untrusted server names so a name padded with newlines
 * cannot break the table layout. This is a layout measure, not a security one:
 * the value is still rendered as text content, never as markup.
 */
export function tidyName(name: string): string {
  return name.replace(/\s+/g, " ").trim();
}

/**
 * The one-line mod readiness summary.
 *
 * Deliberately pure and directly tested, because this is the copy that has to
 * stay honest:
 *
 * - "at least" appears whenever any required mod's size is unresolved, so a
 *   floor is never presented as a total.
 * - An unavailable scan reports that it is unavailable. It never falls through
 *   to a reassuring message.
 * - The closing clause states what actually happens — the game downloads the
 *   missing content during the join — because the launcher cannot do it. There
 *   is no "prepare mods" action to imply.
 */
export function readinessSummary(report: ReadinessReport): string {
  if (!report.available) {
    return report.message ?? "Installed mods could not be checked.";
  }
  if (report.required === 0) {
    return "No mods required.";
  }
  if (report.missing === 0 && report.outdated === 0) {
    return `All ${report.required} mods installed and up to date.`;
  }

  let counts: string;
  if (report.missing > 0 && report.outdated > 0) {
    counts = `${report.missing} missing and ${report.outdated} out of date of ${report.required} mods`;
  } else if (report.missing > 0) {
    counts = `${report.missing} of ${report.required} mods missing`;
  } else {
    counts = `${report.outdated} of ${report.required} mods out of date`;
  }

  if (report.missing > 0) {
    if (report.missingBytes > 0) {
      const size = formatBytes(report.missingBytes, 2);
      // "at least" whenever a required mod's size is unresolved, so a floor is
      // never presented as a total.
      counts +=
        report.unresolvedMods > 0
          ? `, at least ${size} to download`
          : `, ${size} to download`;
    } else if (report.unresolvedMods > 0) {
      counts += ", download size unknown";
    }
  }

  const actor =
    report.missing > 0
      ? "Arma Reforger downloads these when you join."
      : "Arma Reforger updates these when you join.";
  return `${counts}. ${actor}`;
}

/** Severity for the readiness line, driving its colour only. */
export function readinessTone(report: ReadinessReport): "unknown" | "ready" | "action" {
  if (!report.available) return "unknown";
  return report.missing === 0 && report.outdated === 0 ? "ready" : "action";
}

/**
 * The only host the API serves scenario imagery from.
 *
 * Verified across 300 servers: every image URL was `https` on this exact host,
 * and about half of servers publish no image at all.
 */
const IMAGE_HOST = "ar-gcp-cdn.bistudio.com";

/**
 * Returns `url` only if it is safe to put in an `img` tag.
 *
 * Image URLs arrive from the API and are therefore untrusted. This is the first
 * of two layers: the page's CSP `img-src` is the enforcing one and would block
 * anything else regardless, but refusing here means the launcher never even
 * attempts a request to a host it did not expect.
 *
 * Anything that is not `https` on the known CDN host — including `data:`,
 * `javascript:`, a redirect through another origin, or a lookalike domain —
 * returns `null`, which renders as no image.
 */
export function safeImageUrl(url: string | null | undefined): string | null {
  if (!url) return null;
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return null;
  }
  if (parsed.protocol !== "https:") return null;
  // Exact host match: `evil-ar-gcp-cdn.bistudio.com.attacker.test` must not pass.
  if (parsed.hostname !== IMAGE_HOST) return null;
  return parsed.toString();
}


/**
 * Finds a Discord invite in untrusted server text.
 *
 * The pattern is ported verbatim from reforgermods.net's own
 * `discordInviteFromText`, so a server showing a Join Discord button on the site
 * shows one here too.
 *
 * Only the invite **code** is taken from the text; the URL is rebuilt from it.
 * Nothing a server operator writes in their name can steer where the button
 * goes — a name containing `discord.gg/x@evil.test` yields a link to `x`.
 */
export function discordInviteFrom(text: string | null | undefined): {
  code: string;
  url: string;
} | null {
  const match = String(text ?? "").match(
    /(?:https?:\/\/)?(?:www\.)?(?:discord\.gg|discord(?:app)?\.com\/invite)\/([A-Za-z0-9_-]{2,64})(?=$|[\s)\]}<>"'|,;:?!./])/i,
  );
  const code = match?.[1];
  if (!code) return null;
  return { code, url: `https://discord.gg/${encodeURIComponent(code)}` };
}
