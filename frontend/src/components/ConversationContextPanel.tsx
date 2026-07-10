import { useMemo, useState } from "react";
import { X } from "lucide-react";
import { useI18n } from "../i18n";
import type {
  CompanyView,
  MemoThreadMessage,
  PortfolioPosition,
  PortfolioSummary
} from "../types/domain";
import { constellationNodes } from "../pages/homeRules";
import { formatBaseMoney, formatMoney, percent } from "../pages/portfolioRules";
import { EmptyState } from "./EmptyState";

type ContextTab = "portfolio" | "company" | "used";

export function ConversationContextPanel({
  positions,
  summary,
  companyView,
  messages,
  loading,
  mobileOpen,
  onMobileClose
}: {
  positions: PortfolioPosition[];
  summary?: PortfolioSummary;
  companyView?: CompanyView | null;
  messages: MemoThreadMessage[];
  loading: boolean;
  mobileOpen: boolean;
  onMobileClose: () => void;
}) {
  const { t } = useI18n();
  const [tab, setTab] = useState<ContextTab>("portfolio");
  const nodes = useMemo(() => constellationNodes(positions), [positions]);
  const latestContext = [...messages]
    .reverse()
    .find((message) => message.role === "assistant" && message.used_context.length)?.used_context;

  return (
    <aside
      className={`conversation-context-panel mobile-drawer mobile-drawer-right${mobileOpen ? " mobile-open" : ""}`}
      aria-label={t("home.contextTitle")}
    >
      <header className="context-mobile-head">
        <strong>{t("home.contextTitle")}</strong>
        <button type="button" onClick={onMobileClose} title={t("home.closePanel")} aria-label={t("home.closePanel")}>
          <X size={18} />
        </button>
      </header>
      <div className="context-tabs" role="tablist">
        <button type="button" className={tab === "portfolio" ? "active" : ""} onClick={() => setTab("portfolio")}>
          {t("home.contextPortfolio")}
        </button>
        <button type="button" className={tab === "company" ? "active" : ""} onClick={() => setTab("company")}>
          {t("home.contextCompany")}
        </button>
        <button type="button" className={tab === "used" ? "active" : ""} onClick={() => setTab("used")}>
          {t("home.contextUsed")}
        </button>
      </div>

      {tab === "portfolio" ? (
        <div className="context-tab-panel">
          <div className="portfolio-context-stats">
            <div>
              <span>{t("home.totalValue")}</span>
              <strong>{summary ? formatBaseMoney(summary) : loading ? "..." : formatMoney(0, "CNY")}</strong>
            </div>
            <div>
              <span>{t("home.totalPl")}</span>
              <strong className={(summary?.total_unrealized_pnl_base ?? 0) >= 0 ? "positive-text" : "warning-text"}>
                {formatMoney(summary?.total_unrealized_pnl_base ?? 0, summary?.base_currency ?? "CNY")}
              </strong>
            </div>
          </div>
          {nodes.length ? (
            <>
              <svg className="portfolio-constellation" viewBox="0 0 500 380" role="img" aria-label={t("home.constellationLabel")}>
                {nodes.map((node) => (
                  <line key={`${node.id}:line`} x1="250" y1="190" x2={node.x} y2={node.y} stroke={node.color} strokeOpacity="0.22" />
                ))}
                {nodes.map((node) => (
                  <g key={node.id}>
                    <circle cx={node.x} cy={node.y} r={node.radius} fill={node.color} opacity="0.92" />
                    <text x={node.x} y={node.y + 3} textAnchor="middle">{node.symbol.slice(0, 8)}</text>
                    <title>{`${node.label} · ${node.group} · ${percent(node.weight)}`}</title>
                  </g>
                ))}
              </svg>
              <div className="top-weight-list">
                {nodes.slice(0, 6).map((node) => (
                  <div key={node.id}>
                    <span style={{ background: node.color }} />
                    <strong>{node.symbol}</strong>
                    <em>{percent(node.weight)}</em>
                  </div>
                ))}
              </div>
            </>
          ) : (
            <EmptyState title={t("home.noPositionsTitle")}>{t("home.noPositionsBody")}</EmptyState>
          )}
        </div>
      ) : null}

      {tab === "company" ? (
        <div className="context-tab-panel company-view-panel">
          {companyView ? (
            <>
              <header>
                <strong>{companyView.company_name}</strong>
                <span>{companyView.symbol} · v{companyView.current_version}</span>
              </header>
              {companySections(companyView).map(([label, content]) =>
                content ? (
                  <section key={label}>
                    <h3>{label}</h3>
                    <p>{content}</p>
                  </section>
                ) : null
              )}
            </>
          ) : (
            <p className="context-empty-copy">{t("home.noCompanyView")}</p>
          )}
        </div>
      ) : null}

      {tab === "used" ? (
        <div className="context-tab-panel used-context-panel">
          {latestContext?.length ? (
            latestContext.map((item, index) => <div key={index}>{contextLabel(item)}</div>)
          ) : (
            <p className="context-empty-copy">{t("home.noUsedContext")}</p>
          )}
        </div>
      ) : null}
    </aside>
  );
}

function companySections(view: CompanyView): Array<[string, string]> {
  return [
    ["Business quality", view.content.business_quality],
    ["Moat", view.content.moat],
    ["Financials", view.content.financials],
    ["Valuation", view.content.valuation_expectations],
    ["Thesis", view.content.thesis],
    ["Risks", view.content.risks],
    ["Catalysts", view.content.catalysts],
    ["Disconfirming evidence", view.content.disconfirming_evidence],
    ["Open questions", view.content.open_questions.join("\n")]
  ];
}

function contextLabel(item: unknown) {
  if (!item || typeof item !== "object") return String(item);
  const value = item as Record<string, unknown>;
  return String(value.label ?? value.kind ?? "Context");
}
