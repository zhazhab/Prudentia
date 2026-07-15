import { AlertCircle, Check, Pencil, Save, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useI18n } from "../i18n";
import type {
  CompanyView,
  ConversationAction,
  PortfolioPosition
} from "../types/domain";

export function ConversationActionCard({
  action,
  companyView,
  positions,
  busy,
  onEdit,
  onConfirm,
  onReject
}: {
  action: ConversationAction;
  companyView?: CompanyView | null;
  positions: PortfolioPosition[];
  busy: boolean;
  onEdit: (payload: Record<string, unknown>) => Promise<void>;
  onConfirm: () => Promise<void>;
  onReject: () => Promise<void>;
}) {
  const { t } = useI18n();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(() => JSON.stringify(action.payload, null, 2));
  const [error, setError] = useState<string | null>(null);
  const pending = ["proposed", "edited", "failed"].includes(action.status);
  const compact = ["executed", "rejected"].includes(action.status);

  useEffect(() => {
    setDraft(JSON.stringify(action.payload, null, 2));
  }, [action.payload]);

  async function save() {
    try {
      const payload = JSON.parse(draft) as Record<string, unknown>;
      await onEdit(payload);
      setError(null);
      setEditing(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }

  return (
    <article className={`conversation-action-card ${action.status}${compact ? " compact" : ""}`}>
      <header>
        <div>
          <span>{actionTypeLabel(action.action_type, t)}</span>
          <h3>{action.title}</h3>
        </div>
        <ActionStatus action={action} />
      </header>
      {!compact ? (
        <>
          <p>{action.rationale}</p>
          {editing ? (
            <textarea
              className="action-json-editor"
              value={draft}
              rows={12}
              aria-label={t("home.actionPayload")}
              onChange={(event) => setDraft(event.target.value)}
            />
          ) : (
            <ActionPreview action={action} companyView={companyView} positions={positions} />
          )}
          {error || action.error ? (
            <div className="action-error">
              <AlertCircle size={14} />
              <span>{error ?? action.error}</span>
            </div>
          ) : null}
          {pending ? (
            <div className="action-controls">
              <button
                type="button"
                onClick={() => (editing ? void save() : setEditing(true))}
                disabled={busy}
                title={editing ? t("home.actionSaveEdit") : t("home.actionEdit")}
                aria-label={editing ? t("home.actionSaveEdit") : t("home.actionEdit")}
              >
                {editing ? <Save size={16} /> : <Pencil size={16} />}
              </button>
              <button
                type="button"
                className="confirm"
                onClick={() => void onConfirm()}
                disabled={busy || editing}
                title={t("home.actionConfirm")}
                aria-label={t("home.actionConfirm")}
              >
                <Check size={17} />
              </button>
              <button
                type="button"
                onClick={() => void onReject()}
                disabled={busy}
                title={t("home.actionReject")}
                aria-label={t("home.actionReject")}
              >
                <X size={17} />
              </button>
            </div>
          ) : null}
        </>
      ) : null}
    </article>
  );
}

function ActionPreview({
  action,
  companyView,
  positions
}: {
  action: ConversationAction;
  companyView?: CompanyView | null;
  positions: PortfolioPosition[];
}) {
  const { t } = useI18n();
  return (
    <div className="action-preview">
      <div className="action-preview-meta">
        <span>{t("home.actionSourceConversation")}</span>
        {action.target_version != null ? (
          <span>{t("home.actionVersionChange", { from: action.target_version, to: action.target_version + 1 })}</span>
        ) : null}
      </div>
      {action.action_type === "company_view_patch" ? (
        <CompanyPatchPreview action={action} companyView={companyView} />
      ) : action.action_type === "trade_record" ? (
        <TradePreview action={action} positions={positions} />
      ) : action.action_type === "rule_graph_patch" ? (
        <RuleGraphPreview action={action} />
      ) : (
        <pre className="action-payload">{JSON.stringify(action.payload, null, 2)}</pre>
      )}
    </div>
  );
}

function CompanyPatchPreview({
  action,
  companyView
}: {
  action: ConversationAction;
  companyView?: CompanyView | null;
}) {
  const { t } = useI18n();
  const payload = record(action.payload);
  const changes = record(payload.changes);
  const current = companyView && companyView.symbol === payload.symbol
    ? companyView.content
    : undefined;
  const labels: Record<string, string> = {
    business_quality: t("home.companyBusinessQuality"),
    moat: t("home.companyMoat"),
    financials: t("home.companyFinancials"),
    valuation_expectations: t("home.companyValuation"),
    thesis: t("home.companyThesis"),
    risks: t("home.companyRisks"),
    catalysts: t("home.companyCatalysts"),
    disconfirming_evidence: t("home.companyDisconfirming"),
    open_questions: t("home.companyOpenQuestions")
  };
  return (
    <div className="company-patch-preview">
      <strong>{String(payload.company_name ?? payload.symbol ?? "")}</strong>
      {Object.entries(changes).map(([key, value]) => (
        <section className="action-diff-section" key={key}>
          <h4>{labels[key] ?? key}</h4>
          <div className="action-diff-grid">
            <div>
              <span>{t("home.actionBefore")}</span>
              <p>{displayValue(current?.[key as keyof typeof current], t("home.actionNone"))}</p>
            </div>
            <div>
              <span>{t("home.actionAfter")}</span>
              <p>{displayValue(value, t("home.actionNone"))}</p>
            </div>
          </div>
        </section>
      ))}
    </div>
  );
}

function TradePreview({
  action,
  positions
}: {
  action: ConversationAction;
  positions: PortfolioPosition[];
}) {
  const { t } = useI18n();
  const payload = record(action.payload);
  const symbol = String(payload.symbol ?? "");
  const position = positions.find((item) => item.symbol === symbol);
  const side = String(payload.side ?? "");
  const quantity = numeric(payload.quantity);
  const price = numeric(payload.price);
  const fees = numeric(payload.fees);
  const currentQuantity = position?.quantity ?? 0;
  const nextQuantity = side === "sell" ? currentQuantity - quantity : currentQuantity + quantity;
  const nextAverage = side === "buy" && nextQuantity > 0
    ? ((position?.average_cost ?? 0) * currentQuantity + price * quantity + fees) / nextQuantity
    : position?.average_cost ?? price;
  const nativeFlow = side === "sell" ? -(price * quantity - fees) : price * quantity + fees;
  const fxRate = numeric(payload.fx_rate, 0);
  const baseFlow = fxRate > 0 ? nativeFlow * fxRate : null;
  return (
    <div className="trade-preview">
      <dl className="trade-preview-grid">
        <div><dt>{t("home.tradeSecurity")}</dt><dd>{symbol}</dd></div>
        <div><dt>{t("home.tradeSide")}</dt><dd>{side === "sell" ? t("home.tradeSell") : t("home.tradeBuy")}</dd></div>
        <div><dt>{t("home.tradeQuantity")}</dt><dd>{quantity}</dd></div>
        <div><dt>{t("home.tradePrice")}</dt><dd>{`${String(payload.currency ?? "")} ${price.toFixed(2)}`}</dd></div>
        <div><dt>{t("home.tradeDate")}</dt><dd>{String(payload.occurred_at ?? "")}</dd></div>
        <div><dt>{t("home.tradeFx")}</dt><dd>{fxRate > 0 ? `${fxRate} · ${String(payload.fx_source ?? "")}` : String(payload.fx_error ?? t("home.tradeFxMissing"))}</dd></div>
      </dl>
      <section className="trade-impact">
        <h4>{t("home.tradeImpact")}</h4>
        <p>{t("home.tradeQuantityChange", { from: currentQuantity, to: nextQuantity })}</p>
        <p>{t("home.tradeAverageCost", { value: nextAverage.toFixed(2), currency: String(payload.currency ?? position?.currency ?? "") })}</p>
        <p>{baseFlow == null
          ? t("home.tradeCashFlowNative", { value: nativeFlow.toFixed(2), currency: String(payload.currency ?? "") })
          : t("home.tradeCashFlowBase", { value: baseFlow.toFixed(2) })}</p>
      </section>
    </div>
  );
}

function RuleGraphPreview({ action }: { action: ConversationAction }) {
  const { t } = useI18n();
  const payload = record(action.payload);
  const graph = record(payload.graph);
  const nodes = Array.isArray(graph.nodes) ? graph.nodes : [];
  const edges = Array.isArray(graph.edges) ? graph.edges : [];
  return (
    <div className="rule-graph-preview">
      <strong>{String(graph.name ?? t("home.subjectSystem"))}</strong>
      <p>{t("home.ruleGraphShape", { nodes: nodes.length, edges: edges.length })}</p>
      <div className="rule-node-list">
        {nodes.slice(0, 8).map((node, index) => {
          const value = record(node);
          return <span key={String(value.id ?? index)}>{String(value.label ?? value.id ?? value.kind ?? "node")}</span>;
        })}
      </div>
    </div>
  );
}

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
}

function numeric(value: unknown, fallback = 0) {
  const parsed = typeof value === "number" ? value : Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function displayValue(value: unknown, empty: string) {
  if (Array.isArray(value)) return value.length ? value.join("\n") : empty;
  if (value == null || value === "") return empty;
  return typeof value === "string" ? value : JSON.stringify(value, null, 2);
}

function ActionStatus({ action }: { action: ConversationAction }) {
  const { t } = useI18n();
  const label =
    action.status === "executed"
      ? t("home.actionExecuted")
      : action.status === "rejected"
        ? t("home.actionRejected")
        : action.status === "failed"
          ? t("home.actionFailed")
          : null;
  return label ? <span className={`action-status ${action.status}`}>{label}</span> : null;
}

function actionTypeLabel(value: string, t: ReturnType<typeof useI18n>["t"]) {
  if (value === "company_view_patch") return t("home.actionCompanyView");
  if (value === "trade_record") return t("home.actionTrade");
  if (value === "rule_graph_patch") return t("home.actionRuleGraph");
  return value;
}
