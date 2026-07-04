import { useState } from "react";
import { AppShell, type ViewKey } from "./components/AppShell";
import { MemosPage } from "./pages/MemosPage";
import { PortfolioPage } from "./pages/PortfolioPage";
import { SettingsPage } from "./pages/SettingsPage";

export default function App() {
  const [activeView, setActiveView] = useState<ViewKey>("portfolio");

  return (
    <AppShell activeView={activeView} onViewChange={setActiveView}>
      {activeView === "portfolio" ? <PortfolioPage /> : null}
      {activeView === "memos" ? <MemosPage /> : null}
      {activeView === "settings" ? <SettingsPage /> : null}
    </AppShell>
  );
}
