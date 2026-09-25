/** Application preferences and launch history. */

import { useState } from "preact/hooks";
import {
  clearLocalHistory,
  history,
  debugMode,
  installIdentity,
  nowMs,
  saveSettings,
  settings,
} from "../lib/state";
import { launchGame } from "../lib/ipc";
import { formatTimestamp, tidyName } from "../lib/format";
import type { Settings } from "../lib/types";

export function SettingsView() {
  const stored = settings.value;
  const [draft, setDraft] = useState<Settings | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  if (!stored) {
    return <div class="state">Loading settings…</div>;
  }

  const current = draft ?? stored;
  const dirty = draft !== null && JSON.stringify(draft) !== JSON.stringify(stored);

  function patch(next: Partial<Settings>) {
    setDraft({ ...current, ...next });
  }

  async function save() {
    setSaveError(null);
    try {
      await saveSettings(current);
      setDraft(null);
    } catch (err) {
      setSaveError(
        typeof err === "object" && err !== null && "message" in err
          ? String((err as { message: unknown }).message)
          : "Could not save settings.",
      );
    }
  }

  return (
    <div class="page">
      <h2>Settings</h2>
      <p class="page__lede">
        Stored locally in the launcher's configuration directory. No reforgermods.net
        account is required, and the launcher sends no credentials.
      </p>

      <div class="field">
        <span class="field__k">Servers per page</span>
        <input
          type="number"
          min={10}
          max={500}
          value={current.perPage}
          onInput={(e) => patch({ perPage: Number((e.target as HTMLInputElement).value) })}
        />
        <span class="field__hint">Between 10 and 500. Larger pages mean fewer requests.</span>
      </div>

      <div class="field">
        <span class="field__k">Refresh automatically</span>
        <input
          type="checkbox"
          checked={current.autoRefresh}
          onChange={(e) => patch({ autoRefresh: (e.target as HTMLInputElement).checked })}
        />
        <span class="field__hint">
          On by default. The list refreshes only while the window is visible and the
          browser is open, so a backgrounded launcher makes no requests.
        </span>
      </div>

      <div class="field">
        <span class="field__k">Refresh interval</span>
        <input
          type="number"
          min={15}
          max={3600}
          value={current.refreshIntervalSecs}
          disabled={!current.autoRefresh}
          onInput={(e) =>
            patch({ refreshIntervalSecs: Number((e.target as HTMLInputElement).value) })
          }
        />
        <span class="field__hint">Seconds. Floored at 15.</span>
      </div>

      <div class="field">
        <span class="field__k">Discord status</span>
        <input
          type="checkbox"
          checked={current.discordPresence}
          onChange={(e) =>
            patch({ discordPresence: (e.target as HTMLInputElement).checked })
          }
        />
        <span class="field__hint">
          Shows "reforgermods.net launcher" as the application you are running, along
          with the server you are viewing. Turn this off to publish nothing to Discord.
        </span>
      </div>

      <div style={{ display: "flex", gap: "6px", marginTop: "14px" }}>
        <button class="btn" disabled={!dirty} onClick={() => void save()}>
          Save changes
        </button>
        <button class="btn" disabled={!dirty} onClick={() => setDraft(null)}>
          Discard
        </button>
        <button class="btn" onClick={() => void launchGame()} title="steam://run/1874880">
          Launch Arma Reforger
        </button>
      </div>
      {saveError && (
        <p class="state__hint" style={{ color: "var(--bad)" }}>
          {saveError}
        </p>
      )}

      <InstallIdentityPanel />
      <LaunchHistory />
    </div>
  );
}

/**
 * Install identity, shown only in debug mode.
 *
 * The identifier is derived from the machine so it survives a reinstall and the
 * API does not count one person's reinstalls as separate installs. It is not
 * user-facing: there is nothing to configure, and surfacing a stable identifier
 * in the window invites it being copied around.
 */
function InstallIdentityPanel() {
  if (!debugMode.value) return null;
  const identity = installIdentity.value;

  return (
    <div class="section" style={{ marginTop: "26px" }}>
      <div class="section__h">
        <span>Install identity (debug)</span>
      </div>
      <p class="page__lede" style={{ marginBottom: "8px" }}>
        Sent with every API request so reforgermods.net can count distinct
        installs rather than only requests. Derived from this machine, salted and
        hashed — the machine value itself never leaves the process.
      </p>
      <div class="identity">
        <code>{identity?.id || "unavailable"}</code>
        <span class="simple-list__meta">
          {identity?.machineDerived
            ? "machine-derived (survives reinstall)"
            : "random fallback (no machine source readable)"}
        </span>
      </div>
    </div>
  );
}

function LaunchHistory() {
  const records = history.value;
  const now = nowMs.value;

  return (
    <div class="section" style={{ marginTop: "26px" }}>
      <div class="section__h">
        <span>Launch history</span>
        {records.length > 0 && (
          <button class="linkish" onClick={() => void clearLocalHistory()}>
            Clear history and recents
          </button>
        )}
      </div>
      {records.length === 0 ? (
        <p class="state__hint" style={{ margin: 0 }}>
          Servers you join are recorded here.
        </p>
      ) : (
        <div class="simple-list">
          {records.slice(0, 40).map((record, i) => (
            <div class="simple-list__row" key={`${record.serverId}-${record.launchedAt}-${i}`}>
              <span class="simple-list__name">{tidyName(record.serverName) || record.serverId}</span>
              <span class="simple-list__meta">
                {record.succeeded ? "handed to Steam" : "failed"}
              </span>
              <span class="simple-list__meta">{formatTimestamp(record.launchedAt, now)}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
