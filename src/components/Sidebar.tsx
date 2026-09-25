/** Persistent left navigation. */

import { favorites, history, recents, setView, view, type View } from "../lib/state";

interface Item {
  id: View;
  label: string;
  count?: number;
}

export function Sidebar() {
  const items: Item[] = [
    { id: "browse", label: "Server browser" },
    { id: "favorites", label: "Favorites", count: favorites.value.length },
    { id: "recent", label: "Recent", count: recents.value.length },
  ];

  return (
    <nav class="nav" aria-label="Sections">
      <div class="nav__label">Play</div>
      {items.map((item) => (
        <button
          key={item.id}
          class="nav__item"
          aria-current={view.value === item.id}
          onClick={() => setView(item.id)}
        >
          <span>{item.label}</span>
          {item.count !== undefined && item.count > 0 && (
            <span class="nav__count">{item.count}</span>
          )}
        </button>
      ))}

      <div class="nav__label">Launcher</div>
      <button
        class="nav__item"
        aria-current={view.value === "settings"}
        onClick={() => setView("settings")}
      >
        <span>Settings</span>
        {history.value.length > 0 && <span class="nav__count">{history.value.length}</span>}
      </button>

      <div class="nav__spacer" />
      <div class="nav__foot">
        reforgermods.net is not affiliated with Bohemia Interactive.
      </div>
    </nav>
  );
}
