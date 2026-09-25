/**
 * Row status marks and the favourite star.
 *
 * These replace the text pills that used to trail the server name. The pills
 * started wherever the name happened to end, so they formed a ragged column
 * that read as clutter on every row; as icons in a fixed slot they line up and
 * become scannable. Glyphs are Bootstrap Icons (MIT), inlined as paths.
 */

import type { JSX } from "preact";

const LOCK = "M8 0a4 4 0 0 1 4 4v2.05a2.5 2.5 0 0 1 2 2.45v5a2.5 2.5 0 0 1-2.5 2.5h-7A2.5 2.5 0 0 1 2 13.5v-5a2.5 2.5 0 0 1 2-2.45V4a4 4 0 0 1 4-4m0 1a3 3 0 0 0-3 3v2h6V4a3 3 0 0 0-3-3";
const OFFICIAL = "M10.067.87a2.89 2.89 0 0 0-4.134 0l-.622.638-.89-.011a2.89 2.89 0 0 0-2.924 2.924l.01.89-.636.622a2.89 2.89 0 0 0 0 4.134l.637.622-.011.89a2.89 2.89 0 0 0 2.924 2.924l.89-.01.622.636a2.89 2.89 0 0 0 4.134 0l.622-.637.89.011a2.89 2.89 0 0 0 2.924-2.924l-.01-.89.636-.622a2.89 2.89 0 0 0 0-4.134l-.637-.622.011-.89a2.89 2.89 0 0 0-2.924-2.924l-.89.01zm.287 5.984-3 3a.5.5 0 0 1-.708 0l-1.5-1.5a.5.5 0 1 1 .708-.708L7 8.793l2.646-2.647a.5.5 0 0 1 .708.708";
const BATTLEYE = "M5.072.56C6.157.265 7.31 0 8 0s1.843.265 2.928.56c1.11.3 2.229.655 2.887.87a1.54 1.54 0 0 1 1.044 1.262c.596 4.477-.787 7.795-2.465 9.99a11.8 11.8 0 0 1-2.517 2.453 7 7 0 0 1-1.048.625c-.28.132-.581.24-.829.24s-.548-.108-.829-.24a7 7 0 0 1-1.048-.625 11.8 11.8 0 0 1-2.517-2.453C1.928 10.487.545 7.169 1.141 2.692A1.54 1.54 0 0 1 2.185 1.43 63 63 0 0 1 5.072.56";
const OFFLINE = "M16 8A8 8 0 1 1 0 8a8 8 0 0 1 16 0M4.5 7.5a.5.5 0 0 0 0 1h7a.5.5 0 0 0 0-1z";
const STAR_FILLED = "M3.612 15.443c-.386.198-.824-.149-.746-.592l.83-4.73L.173 6.765c-.329-.314-.158-.888.283-.95l4.898-.696L7.538.792c.197-.39.73-.39.927 0l2.184 4.327 4.898.696c.441.062.612.636.282.95l-3.522 3.356.83 4.73c.078.443-.36.79-.746.592L8 13.187l-4.389 2.256z";
const STAR_OUTLINE = "M2.866 14.85c-.078.444.36.791.746.593l4.39-2.256 4.389 2.256c.386.198.824-.149.746-.592l-.83-4.73 3.522-3.356c.33-.314.16-.888-.282-.95l-4.898-.696L8.465.792a.513.513 0 0 0-.927 0L5.354 5.12l-4.898.696c-.441.062-.612.636-.283.95l3.523 3.356-.83 4.73zm4.905-2.767-3.686 1.894.694-3.957a.56.56 0 0 0-.163-.505L1.71 6.745l4.052-.576a.53.53 0 0 0 .393-.288L8 2.223l1.847 3.658a.53.53 0 0 0 .393.288l4.052.575-2.906 2.77a.56.56 0 0 0-.163.506l.694 3.957-3.686-1.894a.5.5 0 0 0-.461 0z";

function Glyph({ path, size }: { path: string; size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d={path} />
    </svg>
  );
}

/**
 * The status marks for one server, in a fixed order.
 *
 * Order is constant so a given mark always occupies the same position in the
 * slot, which is what lets the eye skip the column rather than read it.
 */
export function StatusMarks({ server }: {
  server: {
    official: boolean;
    passwordProtected: boolean;
    battlEye: boolean;
    online: boolean;
  };
}): JSX.Element {
  return (
    <span class="marks">
      {server.official && (
        <span class="marks__i marks__i--official" title="Official server">
          <Glyph path={OFFICIAL} size={12} />
        </span>
      )}
      {server.passwordProtected && (
        <span class="marks__i marks__i--locked" title="Password required">
          <Glyph path={LOCK} size={12} />
        </span>
      )}
      {server.battlEye && (
        <span class="marks__i" title="BattlEye enabled">
          <Glyph path={BATTLEYE} size={12} />
        </span>
      )}
      {!server.online && (
        <span class="marks__i marks__i--offline" title="Not currently listed">
          <Glyph path={OFFLINE} size={12} />
        </span>
      )}
    </span>
  );
}

/** Favourite toggle. */
export function FavouriteStar({
  isFavorite,
  onToggle,
}: {
  isFavorite: boolean;
  onToggle: () => void;
}): JSX.Element {
  return (
    <button
      class="star"
      data-on={isFavorite}
      title={isFavorite ? "Remove from favorites" : "Add to favorites"}
      aria-label={isFavorite ? "Remove from favorites" : "Add to favorites"}
      aria-pressed={isFavorite}
      onClick={(e) => {
        e.stopPropagation();
        onToggle();
      }}
    >
      <Glyph path={isFavorite ? STAR_FILLED : STAR_OUTLINE} size={14} />
    </button>
  );
}
