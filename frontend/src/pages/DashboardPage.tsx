import { useQuery } from "@tanstack/react-query";
import { ArrowRight, BookOpenText, RefreshCw, ShieldCheck, WalletCards } from "lucide-react";
import { Cell, Pie, PieChart, ResponsiveContainer, Tooltip } from "recharts";
import { api } from "../api/client";
import { EmptyState } from "../components/EmptyState";
import { StatCard } from "../components/StatCard";
import type { ViewKey } from "../components/AppShell";
import { useI18n } from "../i18n";

interface DashboardPageProps {
  onNavigate: (view: ViewKey) => void;
}

const chartColors = ["#2f6f73", "#b86b3d", "#6a5f95", "#4f7d45", "#a84f5f"];

export function DashboardPage({ onNavigate }: DashboardPageProps) {
  const { languageTag, t } = useI18n();
  const summary = useQuery({ queryKey: ["portfolio-summary"], queryFn: api.portfolioSummary });
  const memos = useQuery({ queryKey: ["memos"], queryFn: api.memos });
  const profile = useQuery({
    queryKey: ["profile", languageTag],
    queryFn: () => api.profile(languageTag)
  });

  const recentMemos = memos.data?.slice(0, 4) ?? [];
  const topPositions = summary.data?.top_positions ?? [];

  return (
    <div className="page-stack">
      <header className="page-header">
        <div>
          <span className="eyebrow">{t("dashboard.eyebrow")}</span>
          <h2>{t("dashboard.title")}</h2>
        </div>
        <button className="primary-button" type="button" onClick={() => onNavigate("memos")}>
          <BookOpenText size={18} />
          {t("dashboard.newMemo")}
        </button>
      </header>

      <section className="stats-grid">
        <StatCard
          label={t("dashboard.portfolioValue")}
          value={currency(summary.data?.total_market_value ?? 0)}
          detail={t("common.positionsTracked", { count: summary.data?.positions_count ?? 0 })}
          icon={<WalletCards size={18} />}
        />
        <StatCard
          label={t("dashboard.unrealizedPl")}
          value={currency(summary.data?.total_unrealized_pnl ?? 0)}
          detail={t("dashboard.costBasis", { value: currency(summary.data?.total_cost ?? 0) })}
          tone={(summary.data?.total_unrealized_pnl ?? 0) >= 0 ? "positive" : "warning"}
        />
        <StatCard
          label={t("dashboard.profileLevel")}
          value={t("dashboard.level", { level: profile.data?.level ?? 1 })}
          detail={t("dashboard.xpEarned", { xp: profile.data?.xp ?? 0 })}
          icon={<ShieldCheck size={18} />}
        />
        <StatCard
          label={t("dashboard.priceFreshness")}
          value={t("dashboard.staleCount", { count: summary.data?.price_stale_count ?? 0 })}
          detail={t("dashboard.refreshFromPortfolio")}
          tone={(summary.data?.price_stale_count ?? 0) > 0 ? "warning" : "neutral"}
          icon={<RefreshCw size={18} />}
        />
      </section>

      <section className="dashboard-grid">
        <div className="panel">
          <div className="panel-head">
            <h3>{t("dashboard.portfolioWeight")}</h3>
            <button className="ghost-button" type="button" onClick={() => onNavigate("portfolio")}>
              {t("common.open")}
              <ArrowRight size={16} />
            </button>
          </div>
          {topPositions.length ? (
            <div className="chart-row">
              <ResponsiveContainer width="100%" height={220}>
                <PieChart>
                  <Pie
                    data={topPositions}
                    dataKey="value"
                    nameKey="label"
                    innerRadius={58}
                    outerRadius={88}
                    paddingAngle={2}
                  >
                    {topPositions.map((entry, index) => (
                      <Cell key={entry.label} fill={chartColors[index % chartColors.length]} />
                    ))}
                  </Pie>
                  <Tooltip formatter={(value: number) => currency(value)} />
                </PieChart>
              </ResponsiveContainer>
              <div className="legend-list">
                {topPositions.map((slice, index) => (
                  <div className="legend-row" key={slice.label}>
                    <span style={{ background: chartColors[index % chartColors.length] }} />
                    <strong>{slice.label}</strong>
                    <em>{percent(slice.weight)}</em>
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <EmptyState title={t("dashboard.noPortfolioTitle")}>{t("dashboard.noPortfolioBody")}</EmptyState>
          )}
        </div>

        <div className="panel">
          <div className="panel-head">
            <h3>{t("dashboard.recentMemos")}</h3>
            <button className="ghost-button" type="button" onClick={() => onNavigate("memos")}>
              {t("common.open")}
              <ArrowRight size={16} />
            </button>
          </div>
          {recentMemos.length ? (
            <div className="memo-list compact">
              {recentMemos.map((memo) => (
                <article key={memo.id} className="memo-row">
                  <div>
                    <strong>{memo.title}</strong>
                    <p>{memo.symbol ?? memo.asset_type}</p>
                  </div>
                  <span className="pill">{memo.status}</span>
                </article>
              ))}
            </div>
          ) : (
            <EmptyState title={t("dashboard.noMemosTitle")}>{t("dashboard.noMemosBody")}</EmptyState>
          )}
        </div>
      </section>

      <section className="panel">
        <div className="panel-head">
          <h3>{t("dashboard.profileSignals")}</h3>
          <button className="ghost-button" type="button" onClick={() => onNavigate("profile")}>
            {t("common.open")}
            <ArrowRight size={16} />
          </button>
        </div>
        <div className="signal-grid">
          {(profile.data?.attributes ?? []).map((attribute) => (
            <div className="attribute-strip" key={attribute.name}>
              <div>
                <strong>{attribute.name}</strong>
                <p>{attribute.description}</p>
              </div>
              <span>{attribute.score}</span>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

function currency(value: number) {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 0
  }).format(value);
}

function percent(value: number) {
  return `${(value * 100).toFixed(1)}%`;
}
