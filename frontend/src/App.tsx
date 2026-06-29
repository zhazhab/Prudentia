import { useState } from "react";
import { AppShell, type ViewKey } from "./components/AppShell";
import { DashboardPage } from "./pages/DashboardPage";
import { InvestmentSystemPage } from "./pages/InvestmentSystemPage";
import { MemosPage } from "./pages/MemosPage";
import { PortfolioPage } from "./pages/PortfolioPage";
import { ProfilePage } from "./pages/ProfilePage";
import { SettingsPage } from "./pages/SettingsPage";

export default function App() {
  const [activeView, setActiveView] = useState<ViewKey>("dashboard");

  return (
    <AppShell activeView={activeView} onViewChange={setActiveView}>
      {activeView === "dashboard" ? <DashboardPage onNavigate={setActiveView} /> : null}
      {activeView === "portfolio" ? <PortfolioPage /> : null}
      {activeView === "memos" ? <MemosPage /> : null}
      {activeView === "system" ? <InvestmentSystemPage /> : null}
      {activeView === "profile" ? <ProfilePage /> : null}
      {activeView === "settings" ? <SettingsPage /> : null}
    </AppShell>
  );
}
