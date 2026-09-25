/**
 * @vitest-environment jsdom
 *
 * Smoke test for the shell: with the Tauri IPC layer stubbed, the application
 * must mount, render its chrome, and populate the table from a server page.
 * This is the check that a component throws at render time — something neither
 * `tsc` nor the Rust tests can catch.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "preact";
import type { ServerSummary } from "./lib/types";

const server: ServerSummary = {
  id: "73e8fb7f-8789-4d88-8aa8-ea69b9aa092f",
  name: "[OGR2] Old Guard Revival 2 | Ruha Finland",
  scenarioId: "{FAC303E9F1B7CAB2}Missions/OGR_RuhaN-S.conf",
  scenarioName: "[OGR] Ruha Conflict North South",
  scenarioThumbnailUrl:
    "https://ar-gcp-cdn.bistudio.com/image/b64d/02af30860106e7097a8238852468fb85d249062ae56b43b1d6c34e70c20f/45316.jpg",
  gameVersion: "1.8.0.13",
  official: false,
  passwordProtected: false,
  battlEye: true,
  crossPlay: true,
  players: 12,
  maxPlayers: 128,
  queue: 0,
  queueMax: 25,
  pingSiteId: "new_york",
  region: "new_york",
  platform: "LINUX",
  platforms: ["pc", "xbox", "psn"],
  modCount: 122,
  online: true,
  firstSeen: null,
  lastSeen: null,
  analytics: null,
  joinUrl: null,
};

/** Stub the Tauri bridge; nothing in a test environment can reach a backend. */
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (command: string) => {
    switch (command) {
      case "list_servers":
        return {
          servers: [server],
          totalServers: 4999,
          totalPages: 50,
          currentPage: 1,
          filteredOut: 0,
          dataset: {
            warming: false,
            stale: false,
            snapshotAgeSeconds: 18,
            lastCollectionAt: "2026-09-24T06:54:58Z",
            serverCount: 94223,
            onlineServerCount: 4999,
            offlineServerCount: 89224,
          },
        };
      case "get_local_state":
        return {
          settings: {
            perPage: 100,
            autoRefresh: true,
            refreshIntervalSecs: 60,
            discordPresence: true,
          },
          favorites: [server.id],
          recents: [{ id: server.id, name: server.name, lastSeenAt: 1790149200 }],
          history: [],
        };
      case "get_status":
        return {
          apiReachable: true,
          dataset: null,
          reportedStatus: null,
          note: "This API deployment does not serve /v2/status.",
        };
      case "get_server":
        return {
          ...server,
          present: true,
          hostType: "CommunityDs",
          joinable: true,
          host: "208.92.232.141",
          port: 2001,
          fps: 60,
          activity: {
            peak24h: 95,
            avg24h: 33,
            median24h: 22,
            low24h: 0,
            peak7d: 128,
            samples24h: 1286,
            trackingSince: null,
          },
          modSummary: {
            count: 2,
            knownSize: 8_772_055_531,
            knownSizeText: "8.17 GiB",
            unresolvedCount: 1,
          },
          mods: [
            {
              id: "64610AFB74AA9842",
              name: "WCS_Core",
              version: "8.2.0",
              size: 8_772_055_531,
              sizeText: null,
              sizeKnown: true,
              modUrl: null,
            },
            {
              id: "1337C0DE5DABBEEF",
              name: "RHS - Content Pack 01",
              version: "0.16.5208",
              size: 0,
              sizeText: null,
              sizeKnown: false,
              modUrl: null,
            },
          ],
        };
      case "get_servers":
        return [server];
      case "is_debug_mode":
        return false;
      case "get_install_identity":
        return { id: "", machineDerived: false };
      case "get_fleet_now":
        return { onlineServers: 5089, players: 16352, capacity: 224986, queue: 694 };
      case "ping_regions":
        return { new_york: { summary: { bestMs: 70, medianMs: 72, received: 3, sent: 3 } } };
      case "get_install_id":
        return "8f14e45f-ceea-467a-9c2b-1f2a3b4c5d6e";
      case "ping_server":
        return { summary: { bestMs: 70, medianMs: 72, received: 3, sent: 3 } };
      case "get_server_history":
        return {
          serverId: server.id,
          range: "24h",
          bucket: "5m",
          points: [
            { t: 1790149200, avg: 6, min: 5, max: 7, cap: 128 },
            { t: 1790149500, avg: 40, min: 20, max: 95, cap: 128 },
            { t: 1790149800, avg: 12, min: 10, max: 14, cap: 128 },
          ],
        };
      case "get_mod_readiness":
        return {
          available: true,
          required: 136,
          upToDate: 39,
          outdated: 0,
          missing: 96,
          missingBytes: 8_772_055_531,
          unresolvedMods: 1,
          installedTotal: 235,
        };
      case "list_ping_sites":
        return [
          { id: "new_york", label: "New York", regionGroup: "North America", address: null },
        ];
      default:
        return null;
    }
  }),
}));

async function mount() {
  const { App } = await import("./app");
  const { start } = await import("./lib/state");
  const root = document.createElement("div");
  document.body.appendChild(root);
  render(<App />, root);
  await start();
  // Let the resulting signal updates flush.
  await new Promise((resolve) => setTimeout(resolve, 0));
  return root;
}

/**
 * The column headings, in order.
 *
 * Sortable headings are buttons wrapping a label and a direction arrow, so this
 * reads the label rather than the cell's whole text content — which would
 * otherwise pick up the arrow on whichever column is currently sorted.
 */
function headerLabels(root: HTMLElement): (string | null | undefined)[] {
  return Array.from(root.querySelectorAll(".thead > *")).map(
    (cell) => (cell.querySelector(".th-sort__label") ?? cell).textContent,
  );
}

describe("application shell", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
    vi.resetModules();
  });

  it("renders the brand, navigation and status", async () => {
    const root = await mount();
    const text = root.textContent ?? "";
    expect(text).toContain("reforger");
    expect(text).toContain("launcher");
    expect(text).toContain("Server browser");
    expect(text).toContain("Favorites");
    expect(text).toContain("Settings");
    // Status resolved from the stubbed probe.
    expect(text).toContain("API operational");
  });

  it("renders a fetched server into the table", async () => {
    const root = await mount();
    const text = root.textContent ?? "";
    expect(text).toContain("Old Guard Revival 2");
    expect(text).toContain("12 / 128");
    expect(text).toContain("New York");
    // 4,999 matched servers reported in the footer.
    expect(text).toMatch(/4[.,  ]?999/);
  });

  it("exposes the search field with its shortcut hint", async () => {
    const root = await mount();
    const search = root.querySelector<HTMLInputElement>('input[aria-label="Search servers"]');
    expect(search).not.toBeNull();
    expect(root.textContent).toContain("Ctrl F");
  });

  it("renders the filter chips the API supports", async () => {
    const root = await mount();
    const chips = Array.from(root.querySelectorAll(".chip")).map((c) => c.textContent);
    expect(chips).toEqual(["Official", "Modded", "BattlEye", "Password", "Offline"]);
    // All start unset, so no filter is silently applied at launch.
    for (const chip of root.querySelectorAll(".chip")) {
      expect(chip.getAttribute("data-state")).toBe("unset");
    }
  });

  it("shows the empty inspector until a server is selected", async () => {
    const root = await mount();
    expect(root.textContent).toContain("No server selected");
  });

  it("shows region latency in the table and offers sorting by it", async () => {
    const root = await mount();
    expect(headerLabels(root)).toEqual([
      "Server", "", "Players", "Queue", "Ping", "Region", "Mods", "Platform",
    ]);

    // The stub measures new_york, which is this server's region.
    const cell = root.querySelector(".cell.ping");
    expect(cell?.textContent).toBe("72");

    const sortOptions = Array.from(
      root.querySelectorAll<HTMLSelectElement>('select[aria-label="Sort order"]')[0]?.options ?? [],
    ).map((o) => o.value);
    expect(sortOptions).toContain("ping");
  });

  it("sorts from the column headings, marking what only reaches this page", async () => {
    const root = await mount();
    const { clientSort, query } = await import("./lib/state");

    const heading = (label: string) =>
      Array.from(root.querySelectorAll<HTMLButtonElement>(".thead .th-sort")).find(
        (b) => b.querySelector(".th-sort__label")?.textContent === label,
      );

    // Queue is not an ordering the API offers, so it reorders the loaded page
    // and the footer has to say so.
    heading("Queue")?.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(clientSort.value).toEqual({ column: "queue", direction: "desc" });
    expect(root.textContent).toContain("sorted by queue within this page");

    // Clicking it again reverses it, still page-local.
    heading("Queue")?.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(clientSort.value).toEqual({ column: "queue", direction: "asc" });

    // Players is a sort the API serves across every matching server, so the
    // local reordering is dropped and the caveat goes with it.
    heading("Players")?.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(clientSort.value).toBeNull();
    expect(query.value.sort).toBe("players");
    expect(root.textContent).not.toContain("within this page");

    // Reversing it is something the API has no parameter for, so that one does
    // fall back to the page.
    heading("Players")?.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(clientSort.value).toEqual({ column: "players", direction: "asc" });
    expect(root.textContent).toContain("sorted by players within this page");
  });

  it("renders favorites as a server table, not a plain list", async () => {
    const root = await mount();
    const { setView } = await import("./lib/state");
    setView("favorites");
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Same table chrome as the browser, with live columns.
    expect(headerLabels(root)).toEqual([
      "Server", "", "Players", "Queue", "Ping", "Region", "Mods", "Platform",
    ]);
    expect(root.textContent).toContain("Old Guard Revival 2");
    expect(root.textContent).toContain("12 / 128");
    // The inspector travels with the table.
    expect(root.textContent).toContain("No server selected");
  });

  it("keeps the install identifier out of the UI outside debug mode", async () => {
    const root = await mount();
    const { setView } = await import("./lib/state");
    setView("settings");
    await new Promise((resolve) => setTimeout(resolve, 0));

    const text = root.textContent ?? "";
    expect(text).not.toContain("Install identity");
    expect(text).not.toContain("Reset identifier");
    // The Discord toggle is user-facing and should still be there.
    expect(text).toContain("Discord status");
  });

  it("shows a platform mark per supported platform", async () => {
    const root = await mount();
    const badges = Array.from(
      root.querySelectorAll(".row .platforms__badge svg"),
    ).map((b) => b.getAttribute("aria-label"));
    expect(badges).toEqual(["PC", "Xbox", "PlayStation"]);
    expect(root.textContent).not.toContain("XPLAY");
  });

  it("aligns the header with the rows by sharing one scroll container", async () => {
    // With the header outside the scroller, a visible scrollbar narrowed the
    // rows and every column drifted left of its heading.
    const root = await mount();
    const table = root.querySelector(".table");
    expect(table?.querySelector(".thead")).not.toBeNull();
    expect(table?.querySelector(".rows")).not.toBeNull();
  });

  it("shows status as aligned marks, not pills trailing the name", async () => {
    const root = await mount();
    // The pills used to start wherever the name ended, forming a ragged column.
    expect(root.querySelector(".row .name .tag")).toBeNull();
    const marks = Array.from(
      root.querySelectorAll(".row .marks .marks__i"),
    ).map((m) => m.getAttribute("title"));
    // This server is community, open and BattlEye-enabled.
    expect(marks).toEqual(["BattlEye enabled"]);
  });

  it("uses a dedicated favourite button per row", async () => {
    const root = await mount();
    const star = root.querySelector<HTMLButtonElement>(".row .star");
    expect(star).not.toBeNull();
    // The stubbed local state has this server favourited, so the star is on.
    expect(star?.getAttribute("aria-pressed")).toBe("true");
    expect(star?.getAttribute("data-on")).toBe("true");
    expect(star?.getAttribute("title")).toBe("Remove from favorites");
    // Drawn as a glyph, not a text character.
    expect(star?.querySelector("svg")).not.toBeNull();
  });

  it("drops the population bar beside the player count", async () => {
    const root = await mount();
    expect(root.querySelector(".pop__bar")).toBeNull();
    expect(root.querySelector(".row .cell.num")?.textContent).toBe("12 / 128");
  });

  it("shows the fleet player total beside the server count", async () => {
    const root = await mount();
    const context = root.querySelector(".titlebar__context")?.textContent ?? "";
    expect(context).toMatch(/4[.,\s]?999\s*servers/);
    expect(context).toContain("players online");
    expect(context).toMatch(/16[.,\s]?352/);
  });

  it("colours the wordmark like the site", async () => {
    const root = await mount();
    expect(root.querySelector(".wordmark__reforger")?.textContent).toBe("reforger");
    expect(root.querySelector(".wordmark__mods")?.textContent).toBe("mods");
    expect(root.querySelector(".wordmark__net")?.textContent).toBe(".net");
  });

  it("no longer offers a scenario filter", async () => {
    const root = await mount();
    expect(
      root.querySelector('select[aria-label="Scenario (filters the loaded page)"]'),
    ).toBeNull();
  });

  it("renders readiness and the history chart once a server is selected", async () => {
    const root = await mount();
    const { selectServer } = await import("./lib/state");
    await selectServer(server.id);
    await new Promise((resolve) => setTimeout(resolve, 0));

    const text = root.textContent ?? "";
    // The honest copy, end to end through the IPC boundary.
    expect(text).toContain(
      "96 of 136 mods missing, at least 8.17 GiB to download. " +
        "Arma Reforger downloads these when you join.",
    );
    // Counts read as labelled stats rather than a sentence fragment.
    const counts = Array.from(root.querySelectorAll(".readiness__counts div")).map(
      (d) => [d.querySelector("dt")?.textContent, d.querySelector("dd")?.textContent],
    );
    expect(counts).toEqual([
      ["Installed", "39"],
      ["Missing", "96"],
      ["On disk", "235"],
    ]);
    // Readiness needs action, and the state is carried by a dot, not a side edge.
    expect(root.querySelector(".readiness")?.getAttribute("data-tone")).toBe("action");
    expect(root.querySelector(".readiness__dot")).not.toBeNull();

    // The chart drew from the stubbed series.
    const line = root.querySelector(".chart__line");
    expect(line).not.toBeNull();
    expect(line?.getAttribute("d")).toMatch(/^M0,/);
    expect(text).toContain("peak");

    // And no action is offered that the launcher cannot perform.
    expect(text).not.toContain("Prepare mods");

    // Mod rows split name, version and size into aligned columns.
    const first = root.querySelector(".modlist__row");
    expect(first?.querySelector(".modlist__name")?.textContent).toBe("WCS_Core");
    expect(first?.querySelector(".modlist__version")?.textContent).toBe("8.2.0");
    expect(first?.querySelector(".modlist__size")?.textContent).toBe("8.2 GiB");
  });

  it("renders the scenario image and the measured ping", async () => {
    const root = await mount();
    const { selectServer } = await import("./lib/state");
    await selectServer(server.id);
    await new Promise((resolve) => setTimeout(resolve, 0));

    const img = root.querySelector<HTMLImageElement>(".scenario__img");
    expect(img).not.toBeNull();
    expect(img?.getAttribute("src")).toContain("ar-gcp-cdn.bistudio.com");

    // Measured latency, not a region estimate.
    expect(root.textContent).toContain("72 ms");
    // The published address, for people who want to see it.
    expect(root.textContent).toContain("208.92.232.141:2001");
  });

  it("offers the history range switcher", async () => {
    const root = await mount();
    const { selectServer } = await import("./lib/state");
    await selectServer(server.id);
    await new Promise((resolve) => setTimeout(resolve, 0));

    const ranges = Array.from(root.querySelectorAll(".rangeswitch button")).map(
      (b) => b.textContent,
    );
    expect(ranges).toEqual(["6h", "24h", "7d", "30d"]);
    const active = root.querySelector('.rangeswitch button[data-on="true"]');
    expect(active?.textContent).toBe("24h");
  });
});
