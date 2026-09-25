/**
 * Custom window title bar.
 *
 * The native Windows caption plus our own brand strip meant two stacked bars
 * doing one job. This is a single row: brand, context, status, window controls.
 * The window is frameless (`decorations: false`), so dragging and the caption
 * buttons are wired here.
 */

import { getCurrentWindow } from "@tauri-apps/api/window";
import { StatusPill } from "./StatusPill";
import { fleet, totalServers, view } from "../lib/state";
import brandMark from "../assets/brand-mark.png";

/**
 * Resolved lazily: `getCurrentWindow()` reads Tauri internals that only exist
 * inside the app, so calling it at module scope would break any environment
 * that merely imports this file.
 */
function windowAction(action: "minimize" | "toggleMaximize" | "close"): void {
  try {
    void getCurrentWindow()[action]();
  } catch {
    // No Tauri window (tests, or a browser preview): nothing to do.
  }
}

export function TitleBar() {
  return (
    <header class="titlebar" data-tauri-drag-region>
      {/* The drag region must be the element itself, not a child, or the
          draggable area ends up shaped like the text inside it. */}
      <img class="titlebar__mark" src={brandMark} alt="" data-tauri-drag-region />
      {/* Wordmark split and coloured as on reforgermods.net. */}
      <span class="wordmark" data-tauri-drag-region>
        <span class="wordmark__reforger">reforger</span>
        <span class="wordmark__mods">mods</span>
        <span class="wordmark__net">.net</span>
        <em class="wordmark__app">launcher</em>
      </span>

      <span class="titlebar__context" data-tauri-drag-region>
        {view.value === "browse" && totalServers.value > 0
          ? `${totalServers.value.toLocaleString()} servers`
          : ""}
        {fleet.value && fleet.value.players > 0 && (
          <>
            {view.value === "browse" && totalServers.value > 0 && (
              <span class="titlebar__dot" />
            )}
            {`${fleet.value.players.toLocaleString()} players online`}
          </>
        )}
      </span>

      <div class="titlebar__spacer" data-tauri-drag-region />
      <StatusPill />

      <div class="wincontrols">
        <button
          class="wincontrols__btn"
          aria-label="Minimise"
          onClick={() => windowAction("minimize")}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <path d="M0 5h10" stroke="currentColor" stroke-width="1" />
          </svg>
        </button>
        <button
          class="wincontrols__btn"
          aria-label="Maximise"
          onClick={() => windowAction("toggleMaximize")}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <rect
              x="0.5"
              y="0.5"
              width="9"
              height="9"
              fill="none"
              stroke="currentColor"
              stroke-width="1"
            />
          </svg>
        </button>
        <button
          class="wincontrols__btn wincontrols__btn--close"
          aria-label="Close"
          onClick={() => windowAction("close")}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1" />
          </svg>
        </button>
      </div>
    </header>
  );
}
