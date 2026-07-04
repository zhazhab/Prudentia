import type { ReactNode } from "react";
import {
  BarChart3,
  BookOpenText,
  Compass,
  Settings
} from "lucide-react";
import { useI18n, type Locale } from "../i18n";

export type ViewKey = "portfolio" | "memos" | "settings";

interface AppShellProps {
  activeView: ViewKey;
  onViewChange: (view: ViewKey) => void;
  children: ReactNode;
}

const navItems = [
  { key: "portfolio", labelKey: "nav.portfolio", icon: BarChart3 },
  { key: "memos", labelKey: "nav.memos", icon: BookOpenText },
  { key: "settings", labelKey: "nav.settings", icon: Settings }
] as const;

export function AppShell({ activeView, onViewChange, children }: AppShellProps) {
  const { locale, setLocale, t } = useI18n();

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand-block">
          <div className="brand-mark">
            <Compass size={22} />
          </div>
          <div>
            <h1>Prudentia</h1>
            <p>{t("app.subtitle")}</p>
          </div>
        </div>

        <nav className="nav-list" aria-label={t("app.navLabel")}>
          {navItems.map((item) => {
            const Icon = item.icon;
            const label = t(item.labelKey);
            return (
              <button
                key={item.key}
                className={activeView === item.key ? "nav-item active" : "nav-item"}
                type="button"
                onClick={() => onViewChange(item.key)}
                title={label}
              >
                <Icon size={18} />
                <span>{label}</span>
              </button>
            );
          })}
        </nav>

        <div className="language-switcher" aria-label={t("app.languageLabel")}>
          {(["en", "zh"] as Locale[]).map((item) => (
            <button
              key={item}
              className={locale === item ? "active" : ""}
              type="button"
              onClick={() => setLocale(item)}
            >
              {item === "en" ? t("app.langEnglish") : t("app.langChinese")}
            </button>
          ))}
        </div>
      </aside>

      <main className="main-surface">{children}</main>
    </div>
  );
}
