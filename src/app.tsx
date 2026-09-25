/** Application shell: brand, header, navigation and the active view. */

import { Inspector } from "./components/Inspector";
import { TitleBar } from "./components/TitleBar";
import { Sidebar } from "./components/Sidebar";
import { BrowseView } from "./views/BrowseView";
import { FavoritesView, RecentView } from "./views/ListView";
import { SettingsView } from "./views/SettingsView";
import { view } from "./lib/state";

export function App() {
  const current = view.value;
  // The saved lists render the same table as the browser, so they get the same
  // inspector; only settings is full width.
  const showInspector = current !== "settings";

  return (
    <div class="shell">
      <TitleBar />

      <Sidebar />

      <main class={`main${showInspector ? "" : " main--full"}`}>
        {current === "browse" && <BrowseView />}
        {current === "favorites" && <FavoritesView />}
        {current === "recent" && <RecentView />}
        {current === "settings" && <SettingsView />}
        {showInspector && <Inspector />}
      </main>
    </div>
  );
}
