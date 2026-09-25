/**
 * Typed wrappers around the Tauri command surface.
 *
 * Components call these, never `invoke` directly, so every command name and
 * argument shape is declared once.
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  FleetNow,
  HistoryRange,
  InstallIdentity,
  LauncherStatus,
  LocalStateDto,
  PingResult,
  PingSite,
  ReadinessReport,
  ServerDetail,
  ServerHistory,
  ServerSummary,
  ServerListResult,
  ServerQuery,
  Settings,
} from "./types";

export function listServers(query: ServerQuery): Promise<ServerListResult> {
  return invoke("list_servers", { query });
}

export function getServer(id: string): Promise<ServerDetail> {
  return invoke("get_server", { id });
}

/** Resolves server ids to current summaries, dropping any that no longer exist. */
export function getServers(ids: string[]): Promise<ServerSummary[]> {
  return invoke("get_servers", { ids });
}

export function getServerHistory(id: string, range: HistoryRange): Promise<ServerHistory> {
  return invoke("get_server_history", { id, range });
}

/**
 * Compares the server's required mods against the local addon store.
 *
 * Reporting only — there is no command that acts on the result.
 */
export function getModReadiness(id: string): Promise<ReadinessReport> {
  return invoke("get_mod_readiness", { id });
}

/**
 * Measures latency to a server by ICMP echo.
 *
 * Resolves with a result that states why there is no figure when the host does
 * not answer; it does not reject for an unreachable server.
 */
export function pingServer(id: string): Promise<PingResult> {
  return invoke("ping_server", { id });
}

/**
 * Latency to every ping site, keyed by site id.
 *
 * Eight probes cover an entire page, because `/v2/servers` publishes
 * `pingSiteId` per row but no per-server address.
 */
export function pingRegions(): Promise<Record<string, PingResult>> {
  return invoke("ping_regions");
}

/** Players online across the whole fleet. */
export function getFleetNow(): Promise<FleetNow> {
  return invoke("get_fleet_now");
}

export function listPingSites(): Promise<PingSite[]> {
  return invoke("list_ping_sites");
}

export function getStatus(): Promise<LauncherStatus> {
  return invoke("get_status");
}

/** Hands a validated join URI to Steam. Resolves with the URI that was opened. */
export function joinServer(id: string, name: string): Promise<string> {
  return invoke("join_server", { id, name });
}

export function launchGame(): Promise<void> {
  return invoke("launch_game");
}

export function openServerPage(id: string): Promise<void> {
  return invoke("open_server_page", { id });
}

/** Opens a Discord invite by code. The URL is rebuilt in Rust. */
export function openDiscordInvite(code: string): Promise<void> {
  return invoke("open_discord_invite", { code });
}

export function getLocalState(): Promise<LocalStateDto> {
  return invoke("get_local_state");
}

/** Returns the new favorite state for `id`. */
export function toggleFavorite(id: string): Promise<boolean> {
  return invoke("toggle_favorite", { id });
}

/** Returns the settings as stored, after clamping. */
export function updateSettings(settings: Settings): Promise<Settings> {
  return invoke("update_settings", { settings });
}

export function clearHistory(): Promise<void> {
  return invoke("clear_history");
}

/** True when the launcher was started with RFM_DEBUG set. */
export function isDebugMode(): Promise<boolean> {
  return invoke("is_debug_mode");
}

/** Install identity, for debug mode. Empty outside it. */
export function getInstallIdentity(): Promise<InstallIdentity> {
  return invoke("get_install_identity");
}
