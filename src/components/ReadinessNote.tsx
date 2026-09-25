/**
 * Mod readiness, as information only.
 *
 * Deliberately a single line of text and a count strip — not a checklist with a
 * "prepare" button. There is no verified way to install Reforger Workshop
 * content from outside the game, so the launcher states what it knows and what
 * the game will do, and offers no action it cannot perform. See
 * `docs/mod-readiness.md`.
 */

import { readiness } from "../lib/state";
import { readinessSummary, readinessTone } from "../lib/format";

export function ReadinessNote() {
  const report = readiness.value;
  // Null means the check has not answered yet. Rendering nothing is correct:
  // an unknown state must never look like a reassuring one.
  if (!report) return null;

  const tone = readinessTone(report);

  return (
    <div class="readiness" data-tone={tone}>
      <p class="readiness__line">
        {/* A dot carries the state without a coloured edge down the side. */}
        <span class="readiness__dot" />
        {readinessSummary(report)}
      </p>
      {report.available && report.required > 0 && (
        <dl class="readiness__counts">
          <div>
            <dt>Installed</dt>
            <dd>{report.upToDate}</dd>
          </div>
          {report.outdated > 0 && (
            <div>
              <dt>Outdated</dt>
              <dd>{report.outdated}</dd>
            </div>
          )}
          {report.missing > 0 && (
            <div>
              <dt>Missing</dt>
              <dd>{report.missing}</dd>
            </div>
          )}
          <div>
            <dt>On disk</dt>
            <dd>{report.installedTotal}</dd>
          </div>
        </dl>
      )}
    </div>
  );
}
