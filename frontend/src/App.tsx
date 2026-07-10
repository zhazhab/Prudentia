import { useState } from "react";
import { AppShell, type ViewKey } from "./components/AppShell";
import { HomePage } from "./pages/HomePage";
import { MemosPage } from "./pages/MemosPage";
import { PortfolioPage } from "./pages/PortfolioPage";
import { SettingsPage } from "./pages/SettingsPage";

export default function App() {
  const [activeView, setActiveView] = useState<ViewKey>("home");

  return (
    <AppShell activeView={activeView} onViewChange={setActiveView}>
      {activeView === "home" ? <HomePage /> : null}
      {activeView === "portfolio" ? <PortfolioPage /> : null}
      {activeView === "memos" ? <MemosPage /> : null}
      {activeView === "settings" ? <SettingsPage /> : null}
    </AppShell>
  );
}
