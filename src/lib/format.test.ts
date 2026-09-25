import { describe, expect, it } from "vitest";
import {
  discordInviteFrom,
  formatAge,
  formatBytes,
  formatPlayers,
  formatTimestamp,
  humaniseRegion,
  readinessSummary,
  safeImageUrl,
  readinessTone,
  tidyName,
} from "./format";

describe("formatBytes", () => {
  it("uses binary units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1.0 KiB");
    expect(formatBytes(1024 * 1024)).toBe("1.0 MiB");
    // The real figure from a 122-mod server.
    expect(formatBytes(18870301863)).toBe("17.6 GiB");
  });

  it("drops the decimal at three digits so columns stay narrow", () => {
    expect(formatBytes(1024 * 200)).toBe("200 KiB");
    expect(formatBytes(1024 * 99.5)).toBe("99.5 KiB");
  });

  it("handles nonsense input without throwing", () => {
    expect(formatBytes(-1)).toBe("0 B");
    expect(formatBytes(Number.NaN)).toBe("0 B");
    expect(formatBytes(Number.POSITIVE_INFINITY)).toBe("0 B");
  });
});

describe("formatAge", () => {
  it("renders the header's freshness label", () => {
    expect(formatAge(18)).toBe("18s ago");
    expect(formatAge(0.4)).toBe("just now");
    expect(formatAge(59.9)).toBe("59s ago");
    expect(formatAge(60)).toBe("1m ago");
    expect(formatAge(3600)).toBe("1h ago");
    expect(formatAge(86_400)).toBe("1d ago");
  });

  it("reports unknown rather than a negative age", () => {
    expect(formatAge(-5)).toBe("unknown");
    expect(formatAge(Number.NaN)).toBe("unknown");
  });
});

describe("formatTimestamp", () => {
  it("is relative to the supplied clock", () => {
    const now = 1_700_000_000_000;
    expect(formatTimestamp(now / 1000 - 120, now)).toBe("2m ago");
  });
});

describe("humaniseRegion", () => {
  it("turns ping-site ids into labels", () => {
    expect(humaniseRegion("new_york")).toBe("New York");
    expect(humaniseRegion("frankfurt")).toBe("Frankfurt");
    expect(humaniseRegion("los_angeles")).toBe("Los Angeles");
  });

  it("falls back for an absent region", () => {
    expect(humaniseRegion(null)).toBe("—");
    expect(humaniseRegion("")).toBe("—");
    expect(humaniseRegion(undefined)).toBe("—");
  });
});


describe("formatPlayers", () => {
  it("renders the population cell", () => {
    expect(formatPlayers(12, 128)).toBe("12 / 128");
  });
});


describe("tidyName", () => {
  it("collapses whitespace in untrusted server names", () => {
    expect(tidyName("  [EU1]   Some   Server \n\t ")).toBe("[EU1] Some Server");
  });

  it("leaves ordinary names alone", () => {
    expect(tidyName("[OGR2] Old Guard Revival 2")).toBe("[OGR2] Old Guard Revival 2");
  });

  it("does not attempt to sanitise markup, which is rendered as text", () => {
    // The value is set as text content, so angle brackets are inert; the
    // function's job is layout, not escaping.
    expect(tidyName("<b>x</b>")).toBe("<b>x</b>");
  });
});

describe("readinessSummary", () => {
  it("reads as sentences, not dot-separated fragments", () => {
    // The joined-with-middots form read as machine output.
    expect(readinessSummary(base)).not.toContain("·");
  });

  const base = {
    available: true,
    required: 136,
    upToDate: 39,
    outdated: 0,
    missing: 96,
    missingBytes: 8_772_055_531,
    unresolvedMods: 1,
    installedTotal: 235,
  };

  it("produces the intended sentence for a partly-installed server", () => {
    // "at least" because one mod's size is unresolved; the closing clause
    // states what the game does, because the launcher cannot do it.
    expect(readinessSummary(base)).toBe(
      "96 of 136 mods missing, at least 8.17 GiB to download. " +
        "Arma Reforger downloads these when you join.",
    );
  });

  it("drops 'at least' once every size is resolved", () => {
    const summary = readinessSummary({ ...base, unresolvedMods: 0 });
    expect(summary).toContain("8.17 GiB to download");
    expect(summary).not.toContain("at least");
  });

  it("never presents an unavailable scan as a verdict", () => {
    const summary = readinessSummary({
      available: false,
      unavailable: "no_addons_directory",
      message: "No local Arma Reforger addon folder found, so installed mods could not be checked.",
      required: 0,
      upToDate: 0,
      outdated: 0,
      missing: 0,
      missingBytes: 0,
      unresolvedMods: 0,
      installedTotal: 0,
    });
    expect(summary).toContain("could not be checked");
    expect(summary).not.toContain("up to date");
    expect(summary).not.toContain("missing");
  });

  it("falls back to generic copy if the backend sent no message", () => {
    const summary = readinessSummary({ ...base, available: false, message: undefined });
    expect(summary).toBe("Installed mods could not be checked.");
  });

  it("reports a fully-installed server plainly", () => {
    expect(
      readinessSummary({ ...base, upToDate: 136, outdated: 0, missing: 0, missingBytes: 0, unresolvedMods: 0 }),
    ).toBe("All 136 mods installed and up to date.");
  });

  it("says update, not download, when only versions differ", () => {
    const summary = readinessSummary({
      ...base,
      upToDate: 135,
      outdated: 1,
      missing: 0,
      missingBytes: 0,
      unresolvedMods: 0,
    });
    expect(summary).toBe("1 of 136 mods out of date. Arma Reforger updates these when you join.");
  });

  it("combines missing and outdated counts", () => {
    const summary = readinessSummary({ ...base, outdated: 1, unresolvedMods: 0 });
    expect(summary).toContain("96 missing and 1 out of date of 136 mods");
  });

  it("says the size is unknown rather than showing 0 B", () => {
    const summary = readinessSummary({ ...base, missingBytes: 0, unresolvedMods: 96 });
    expect(summary).toContain("download size unknown");
    expect(summary).not.toContain("0 B");
  });

  it("handles a vanilla server", () => {
    expect(readinessSummary({ ...base, required: 0, missing: 0, missingBytes: 0, unresolvedMods: 0 })).toBe(
      "No mods required.",
    );
  });
});

describe("readinessTone", () => {
  const base = {
    available: true,
    required: 10,
    upToDate: 10,
    outdated: 0,
    missing: 0,
    missingBytes: 0,
    unresolvedMods: 0,
    installedTotal: 20,
  };

  it("never reads as ready when the scan did not run", () => {
    expect(readinessTone({ ...base, available: false })).toBe("unknown");
  });

  it("distinguishes ready from needing action", () => {
    expect(readinessTone(base)).toBe("ready");
    expect(readinessTone({ ...base, missing: 1 })).toBe("action");
    expect(readinessTone({ ...base, outdated: 1 })).toBe("action");
  });
});

describe("formatBytes precision override", () => {
  it("keeps the compact default for table cells", () => {
    expect(formatBytes(18870301863)).toBe("17.6 GiB");
  });

  it("honours an explicit precision for download figures", () => {
    expect(formatBytes(8_772_055_531, 2)).toBe("8.17 GiB");
    expect(formatBytes(8_772_055_531, 0)).toBe("8 GiB");
  });

  it("never puts decimals on whole bytes", () => {
    expect(formatBytes(500, 2)).toBe("500 B");
  });
});

describe("safeImageUrl", () => {
  const real =
    "https://ar-gcp-cdn.bistudio.com/image/b64d/02af30860106e7097a8238852468fb85d249062ae56b43b1d6c34e70c20f/45316.jpg";

  it("accepts the CDN the API actually uses", () => {
    // Verified across 300 servers: every image URL was https on this host.
    expect(safeImageUrl(real)).toBe(real);
  });

  it("treats an absent image as absent, not an error", () => {
    // About half of servers publish no imagery at all.
    expect(safeImageUrl(null)).toBeNull();
    expect(safeImageUrl(undefined)).toBeNull();
    expect(safeImageUrl("")).toBeNull();
  });

  it("refuses non-https schemes", () => {
    expect(safeImageUrl("http://ar-gcp-cdn.bistudio.com/a.jpg")).toBeNull();
    expect(safeImageUrl("data:image/png;base64,iVBORw0KGgo=")).toBeNull();
    expect(safeImageUrl("javascript:alert(1)")).toBeNull();
    expect(safeImageUrl("file:///etc/passwd")).toBeNull();
  });

  it("refuses lookalike and subdomain-suffix hosts", () => {
    for (const url of [
      "https://evil.test/a.jpg",
      "https://ar-gcp-cdn.bistudio.com.attacker.test/a.jpg",
      "https://notar-gcp-cdn.bistudio.com/a.jpg",
      "https://sub.ar-gcp-cdn.bistudio.com/a.jpg",
      "https://ar-gcp-cdn.bistudio.com@evil.test/a.jpg",
    ]) {
      expect(safeImageUrl(url), url).toBeNull();
    }
  });

  it("refuses unparseable input", () => {
    expect(safeImageUrl("not a url")).toBeNull();
    expect(safeImageUrl("//ar-gcp-cdn.bistudio.com/a.jpg")).toBeNull();
  });
});


describe("discordInviteFrom", () => {
  it("finds invites in real server names", () => {
    for (const [name, code] of [
      ["[OGR2] Old Guard Revival | discord.gg/oldguardarma", "oldguardarma"],
      ["[AU] FTA #2 | From The Ashes | discord.gg/from-the-ashes", "from-the-ashes"],
      ["EXD.gg 1PP | https://discord.gg/AbC_123", "AbC_123"],
      ["Server | https://www.discord.com/invite/xyz789", "xyz789"],
      ["Server | discordapp.com/invite/legacy1", "legacy1"],
    ] as const) {
      expect(discordInviteFrom(name)?.code, name).toBe(code);
    }
  });

  it("rebuilds the URL from the code rather than trusting the text", () => {
    const found = discordInviteFrom("Join us at discord.gg/realcode today");
    expect(found?.code).toBe("realcode");
    expect(found?.url).toBe("https://discord.gg/realcode");
  });

  it("refuses a code followed by characters that could redirect it", () => {
    // `discord.gg/code@evil.test` is not an invite; the trailing-boundary rule
    // rejects it outright rather than extracting a code from it.
    expect(discordInviteFrom("discord.gg/realcode@evil.test/path")).toBeNull();
    expect(discordInviteFrom("discord.gg/realcode%2Fevil")).toBeNull();
  });

  it("returns nothing when there is no invite", () => {
    for (const name of [
      "",
      "[EU1] Plain Server Name",
      "discord.gg/",
      "discord.gg/x",
      "https://example.com/invite/abcdef",
    ]) {
      expect(discordInviteFrom(name), name).toBeNull();
    }
    expect(discordInviteFrom(null)).toBeNull();
    expect(discordInviteFrom(undefined)).toBeNull();
  });

  it("matches the site's regex, including its lack of a left boundary", () => {
    // Ported verbatim from reforgermods.net so the button appears on exactly
    // the same servers. A lookalike host is a known false positive upstream;
    // it is harmless here because the URL is rebuilt from the code alone.
    expect(discordInviteFrom("not-discord.gg/abcdef")?.url).toBe(
      "https://discord.gg/abcdef",
    );
  });

  it("stops the code at a trailing separator", () => {
    expect(discordInviteFrom("come to discord.gg/abc123, we play daily")?.code).toBe("abc123");
    expect(discordInviteFrom("discord.gg/abc123.")?.code).toBe("abc123");
    expect(discordInviteFrom("(discord.gg/abc123)")?.code).toBe("abc123");
  });
});
