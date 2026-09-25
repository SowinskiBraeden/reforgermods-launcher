/**
 * Mirrors of the Rust DTOs crossing the Tauri boundary.
 *
 * These are hand-written rather than generated: the surface is small, and
 * keeping it hand-written means the field names the UI reads are visible in
 * one place. `rfm-core` serialises with `rename_all = "camelCase"`, with one
 * deliberate exception — `battlEye`, which matches the API's own spelling.
 *
 * Every string here is untrusted remote text. Render it as text content only;
 * never build a URL or markup from it.
 */

export interface DatasetMeta {
  warming: boolean;
  stale: boolean;
  snapshotAgeSeconds: number;
  lastCollectionAt: string | null;
  serverCount: number;
  onlineServerCount: number;
  offlineServerCount: number;
}

export interface ServerAnalytics {
  rank: number | null;
  overallScore: number | null;
  activityScore: number | null;
  reliabilityScore: number | null;
  confidence: number | null;
}

export interface ServerSummary {
  id: string;
  name: string;
  scenarioId: string;
  scenarioName: string;
  scenarioThumbnailUrl: string | null;
  gameVersion: string;
  official: boolean;
  passwordProtected: boolean;
  /** Spelled as the API spells it. */
  battlEye: boolean;
  crossPlay: boolean;
  players: number;
  maxPlayers: number;
  queue: number;
  queueMax: number;
  pingSiteId: string | null;
  region: string | null;
  /** Host OS (`LINUX`/`WINDOWS`), not a client platform. */
  platform: string | null;
  /** Client platforms that may join: `pc`, `xbox`, `psn`. */
  platforms: string[];
  modCount: number;
  online: boolean;
  firstSeen: string | null;
  lastSeen: string | null;
  analytics: ServerAnalytics | null;
  /** The API's own join URI. Cross-checked in tests; never handed to the OS. */
  joinUrl: string | null;
}

export interface ServerActivity {
  peak24h: number;
  avg24h: number;
  median24h: number;
  low24h: number;
  peak7d: number;
  samples24h: number;
  trackingSince: string | null;
}

export interface ServerModSummary {
  count: number;
  knownSize: number;
  knownSizeText: string | null;
  unresolvedCount: number;
}

export interface ServerMod {
  id: string;
  name: string;
  version: string;
  size: number;
  sizeText: string | null;
  sizeKnown: boolean;
  modUrl: string | null;
}

/** `ServerDetail` flattens `ServerSummary`, so it carries every summary field. */
export interface ServerDetail extends ServerSummary {
  present: boolean;
  hostType: string | null;
  joinable: boolean;
  /** Published address. Untrusted; only the backend ever acts on it. */
  host: string | null;
  port: number;
  /** Server-reported simulation FPS; 0 when not reported. */
  fps: number;
  activity: ServerActivity | null;
  modSummary: ServerModSummary;
  mods: ServerMod[];
}

export interface PingSite {
  id: string;
  label: string;
  regionGroup: string;
  address: string | null;
}

/** Fleet-wide population right now. */
export interface FleetNow {
  onlineServers: number;
  players: number;
  capacity: number;
  queue: number;
}

export interface LauncherStatus {
  apiReachable: boolean;
  dataset: DatasetMeta | null;
  reportedStatus: string | null;
  note: string | null;
}

/** Ranges `/v2/servers/{id}/history` accepts. */
export type HistoryRange =
  | "sixHours"
  | "oneDay"
  | "threeDays"
  | "oneWeek"
  | "oneMonth"
  | "threeMonths"
  | "oneYear"
  | "all";

/** The ranges the inspector offers, with their labels. */
export const HISTORY_RANGES: ReadonlyArray<{ value: HistoryRange; label: string }> = [
  { value: "sixHours", label: "6h" },
  { value: "oneDay", label: "24h" },
  { value: "oneWeek", label: "7d" },
  { value: "oneMonth", label: "30d" },
];

export interface ServerHistoryPoint {
  /** Bucket start, unix seconds. */
  t: number;
  avg: number;
  min: number;
  max: number;
  /** Slot capacity during the bucket; 0 when not recorded. */
  cap: number;
}

export interface ServerHistory {
  serverId: string;
  /** The range the API served, which may differ from what was asked. */
  range: string;
  /** Bucket width the API chose: `5m`, `hour` or `day`. */
  bucket: string;
  points: ServerHistoryPoint[];
}

/** Diagnostic view of the install identity. Populated only in debug mode. */
export interface InstallIdentity {
  id: string;
  /** True when the identifier survives a reinstall. */
  machineDerived: boolean;
}

/** Why a server could not be measured. */
export type PingUnavailable = "no_host" | "unroutable_host" | "no_reply" | "not_permitted";

export interface PingSummary {
  bestMs: number;
  /** Preferred for display: one delayed packet should not move the figure. */
  medianMs: number;
  received: number;
  sent: number;
}

/** A latency measurement, or a stated reason there is none. */
export interface PingResult {
  summary?: PingSummary;
  unavailable?: PingUnavailable;
  message?: string;
}

/** Why a local mod scan could not produce an answer. */
export type ReadinessUnavailable = "no_addons_directory" | "scan_failed";

/**
 * Comparison of a server's required mods against the local addon store.
 *
 * Reporting only. `available: false` means the scan did not run, and every
 * count is then zero and must not be rendered.
 */
export interface ReadinessReport {
  available: boolean;
  unavailable?: ReadinessUnavailable;
  message?: string;
  required: number;
  upToDate: number;
  outdated: number;
  missing: number;
  /** Floor, not a total, while `unresolvedMods` is non-zero. */
  missingBytes: number;
  unresolvedMods: number;
  installedTotal: number;
}

export type ServerSort = "players" | "name" | "newest" | "lastSeen";
export type ClientPlatform = "pc" | "xbox" | "psn";

export interface ServerQuery {
  search: string;
  sort: ServerSort;
  region: string | null;
  platform: ClientPlatform | null;
  official: boolean | null;
  hasMods: boolean | null;
  battleye: boolean | null;
  /** The API's name for password-protected. */
  locked: boolean | null;
  includeOffline: boolean;
  /** Applied client-side; the API has no equivalent parameter. */
  minPlayers: number;
  page: number;
  perPage: number;
}

export interface ServerListResult {
  servers: ServerSummary[];
  totalServers: number;
  totalPages: number;
  currentPage: number;
  /** Rows the client-side filters removed from this page. */
  filteredOut: number;
  dataset: DatasetMeta;
}

export interface Settings {
  perPage: number;
  autoRefresh: boolean;
  refreshIntervalSecs: number;
  /** Publish activity to Discord Rich Presence. */
  discordPresence: boolean;
}

export interface RecentServer {
  id: string;
  name: string;
  lastSeenAt: number;
}

export interface LaunchRecord {
  serverId: string;
  serverName: string;
  launchedAt: number;
  succeeded: boolean;
}

export interface LocalStateDto {
  settings: Settings;
  favorites: string[];
  recents: RecentServer[];
  history: LaunchRecord[];
}

/** The shape every failing command rejects with. */
export interface CommandError {
  kind: string;
  message: string;
  retryAfterSecs?: number;
}

/** Type guard for the error shape, since `catch` gives `unknown`. */
export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as CommandError).kind === "string" &&
    typeof (value as CommandError).message === "string"
  );
}

export const DEFAULT_QUERY: ServerQuery = {
  search: "",
  sort: "players",
  region: null,
  platform: null,
  official: null,
  hasMods: null,
  battleye: null,
  locked: null,
  includeOffline: false,
  minPlayers: 0,
  page: 1,
  perPage: 100,
};
