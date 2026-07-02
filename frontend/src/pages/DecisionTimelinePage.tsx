import { FormEvent, useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, GitFork, RefreshCw, Save, TrendingUp, TriangleAlert } from "lucide-react";
import { api } from "../api/client";
import { EmptyState } from "../components/EmptyState";
import { StatCard } from "../components/StatCard";
import { useI18n, type TranslationKey } from "../i18n";
import type {
  DecisionDeltaDetail,
  DecisionDeltaLeg,
  DecisionDeltaTimelineFilters,
  DecisionDeltaTimelineItem
} from "../types/domain";
import {
  comparisonKindForAction,
  decisionDeltaTone,
  formatDecisionDeltaMoney,
  formatDecisionDeltaPercent,
  isDecisionDeltaStale,
  singleForkLegs,
  summarizeVisibleDecisionDeltas
} from "./decisionDeltaRules";

const snapshotHistoryLimit = 90;

const emptyReviewForm = {
  notes: "",
  thesis_evidence: "",
  disconfirming_evidence: "",
  lessons: "",
  candidate_principles: "",
  candidate_checklist_items: ""
};

type ReviewForm = typeof emptyReviewForm;

export function DecisionTimelinePage() {
  const { languageTag, t } = useI18n();
  const queryClient = useQueryClient();
  const [filters, setFilters] = useState<DecisionDeltaTimelineFilters>({ sort: "date" });
  const [symbolInput, setSymbolInput] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [reviewForm, setReviewForm] = useState<ReviewForm>(emptyReviewForm);
  const [selectedPrinciples, setSelectedPrinciples] = useState<string[]>([]);
  const [selectedChecklist, setSelectedChecklist] = useState<string[]>([]);

  const timeline = useQuery({
    queryKey: ["decision-delta-timeline", filters],
    queryFn: () => api.decisionDeltaTimeline(filters)
  });
  const items = timeline.data?.items ?? [];
  const localSummary = useMemo(() => summarizeVisibleDecisionDeltas(items), [items]);
  const selectedItem = items.find((item) => item.decision.id === selectedId) ?? null;

  const detail = useQuery({
    queryKey: ["decision-delta-detail", selectedId],
    queryFn: () => api.decisionDeltaDetail(selectedId ?? "", snapshotHistoryLimit),
    enabled: Boolean(selectedId)
  });

  useEffect(() => {
    const timeout = window.setTimeout(() => {
      setFilters((current) => ({
        ...current,
        symbol: symbolInput.trim() ? symbolInput.trim().toUpperCase() : undefined
      }));
    }, 250);

    return () => window.clearTimeout(timeout);
  }, [symbolInput]);

  useEffect(() => {
    if (!items.length) {
      setSelectedId(null);
      return;
    }
    if (!selectedId || !items.some((item) => item.decision.id === selectedId)) {
      setSelectedId(items[0].decision.id);
    }
  }, [items, selectedId]);

  useEffect(() => {
    const review = detail.data?.review;
    setReviewForm(
      review
        ? {
            notes: review.notes,
            thesis_evidence: review.thesis_evidence.join("\n"),
            disconfirming_evidence: review.disconfirming_evidence.join("\n"),
            lessons: review.lessons.join("\n"),
            candidate_principles: review.candidate_principles.join("\n"),
            candidate_checklist_items: review.candidate_checklist_items.join("\n")
          }
        : emptyReviewForm
    );
    setSelectedPrinciples([]);
    setSelectedChecklist([]);
  }, [detail.data?.decision.id, detail.data?.review]);

  const refreshDeltas = useMutation({
    mutationFn: () => api.refreshDecisionDeltas(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["decision-delta-timeline"] });
      queryClient.invalidateQueries({ queryKey: ["decision-delta-detail"] });
    }
  });

  const saveReview = useMutation({
    mutationFn: () =>
      api.saveDecisionDeltaReview(
        selectedId ?? "",
        {
          notes: reviewForm.notes.trim(),
          thesis_evidence: splitLines(reviewForm.thesis_evidence),
          disconfirming_evidence: splitLines(reviewForm.disconfirming_evidence),
          lessons: splitLines(reviewForm.lessons),
          candidate_principles: splitLines(reviewForm.candidate_principles),
          candidate_checklist_items: splitLines(reviewForm.candidate_checklist_items)
        },
        languageTag
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["decision-delta-detail", selectedId] });
      queryClient.invalidateQueries({ queryKey: ["decision-delta-timeline"] });
      queryClient.invalidateQueries({ queryKey: ["profile"] });
    }
  });

  const adoptCandidates = useMutation({
    mutationFn: () =>
      api.adoptDecisionDeltaCandidates(
        selectedId ?? "",
        {
          principles: selectedPrinciples,
          checklist_items: selectedChecklist
        },
        languageTag
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["investment-system"] });
      setSelectedPrinciples([]);
      setSelectedChecklist([]);
    }
  });

  const actionMessage =
    refreshDeltas.error?.message ??
    saveReview.error?.message ??
    adoptCandidates.error?.message ??
    (refreshDeltas.data
      ? t("decisionDelta.refreshResult", {
          refreshed: refreshDeltas.data.refreshed,
          failed: refreshDeltas.data.failed
        })
      : saveReview.isSuccess
        ? t("decisionDelta.saved")
        : adoptCandidates.isSuccess
          ? t("decisionDelta.adopted")
          : "");

  const summary = timeline.data?.summary;
  const sumDelta = summary?.sum_delta_value ?? localSummary.sumDeltaValue;
  const impact = summary?.sum_portfolio_impact_pct ?? null;
  const lastRefresh = summary?.last_refreshed_at ?? localSummary.lastRefreshedAt;

  function updateFilter(key: keyof DecisionDeltaTimelineFilters, value: string) {
    setFilters((current) => ({
      ...current,
      [key]: value || undefined
    }));
  }

  function submitReview(event: FormEvent) {
    event.preventDefault();
    if (selectedId) {
      saveReview.mutate();
    }
  }

  return (
    <div className="page-stack">
      <header className="page-header">
        <div>
          <span className="eyebrow">{t("decisionDelta.eyebrow")}</span>
          <h2>{t("decisionDelta.title")}</h2>
        </div>
        <button
          className="primary-button"
          type="button"
          onClick={() => refreshDeltas.mutate()}
          disabled={refreshDeltas.isPending}
        >
          <RefreshCw size={18} />
          {refreshDeltas.isPending ? t("decisionDelta.refreshing") : t("decisionDelta.refresh")}
        </button>
      </header>

      <section className="stats-grid">
        <StatCard
          label={t("decisionDelta.visibleDelta")}
          value={formatDecisionDeltaMoney(sumDelta, "CNY")}
          detail={t("decisionDelta.sumDetail", {
            positive: summary?.positive_delta_count ?? localSummary.positiveDeltaCount,
            negative: summary?.negative_delta_count ?? localSummary.negativeDeltaCount
          })}
          tone={decisionDeltaTone({ delta_value: sumDelta })}
          icon={<TrendingUp size={18} />}
        />
        <StatCard
          label={t("decisionDelta.decisionCount")}
          value={`${summary?.visible_decisions_count ?? localSummary.visibleDecisionsCount}`}
          detail={`${summary?.quantifiable_decisions_count ?? localSummary.quantifiableDecisionsCount} ${t("decisionDelta.quantified")}`}
          icon={<GitFork size={18} />}
        />
        <StatCard
          label={t("decisionDelta.staleSnapshots")}
          value={`${localSummary.staleCount}`}
          detail={impact == null ? undefined : t("decisionDelta.impactDetail", { value: formatDecisionDeltaPercent(impact) })}
          tone={localSummary.staleCount > 0 ? "warning" : "neutral"}
          icon={<TriangleAlert size={18} />}
        />
        <StatCard
          label={t("decisionDelta.lastRefresh")}
          value={lastRefresh ? formatDate(lastRefresh) : t("decisionDelta.never")}
          detail={actionMessage || undefined}
          icon={<Check size={18} />}
        />
      </section>

      <section className="panel decision-delta-filters">
        <div className="panel-head">
          <h3>{t("decisionDelta.filters")}</h3>
        </div>
        <label>
          <span>{t("decisionDelta.symbol")}</span>
          <input
            value={symbolInput}
            onChange={(event) => setSymbolInput(event.target.value.toUpperCase())}
            placeholder={t("decisionDelta.allSymbols")}
          />
        </label>
        <label>
          <span>{t("decisionDelta.action")}</span>
          <select value={filters.action ?? ""} onChange={(event) => updateFilter("action", event.target.value)}>
            <option value="">{t("decisionDelta.allActions")}</option>
            <option value="buy">Buy</option>
            <option value="add">Add</option>
            <option value="sell">Sell</option>
            <option value="trim">Trim</option>
            <option value="watch">Watch</option>
            <option value="skip">Skip</option>
          </select>
        </label>
        <label>
          <span>{t("decisionDelta.delta")}</span>
          <select value={filters.delta ?? ""} onChange={(event) => updateFilter("delta", event.target.value)}>
            <option value="">{t("decisionDelta.allDeltas")}</option>
            <option value="positive">{t("decisionDelta.positive")}</option>
            <option value="negative">{t("decisionDelta.negative")}</option>
            <option value="none">{t("decisionDelta.noSnapshot")}</option>
          </select>
        </label>
        <label>
          <span>{t("decisionDelta.stale")}</span>
          <select value={filters.stale ?? ""} onChange={(event) => updateFilter("stale", event.target.value)}>
            <option value="">{t("decisionDelta.allFreshness")}</option>
            <option value="true">{t("decisionDelta.staleOnly")}</option>
            <option value="false">{t("decisionDelta.freshOnly")}</option>
          </select>
        </label>
        <label>
          <span>{t("decisionDelta.review")}</span>
          <select value={filters.reviewed ?? ""} onChange={(event) => updateFilter("reviewed", event.target.value)}>
            <option value="">{t("decisionDelta.allReviews")}</option>
            <option value="true">{t("decisionDelta.reviewedOnly")}</option>
            <option value="false">{t("decisionDelta.unreviewedOnly")}</option>
          </select>
        </label>
        <label>
          <span>{t("decisionDelta.sort")}</span>
          <select value={filters.sort ?? "date"} onChange={(event) => updateFilter("sort", event.target.value)}>
            <option value="date">{t("decisionDelta.sortDate")}</option>
            <option value="absolute_delta">{t("decisionDelta.sortAbsoluteDelta")}</option>
            <option value="portfolio_impact">{t("decisionDelta.sortImpact")}</option>
            <option value="stale">{t("decisionDelta.sortStale")}</option>
          </select>
        </label>
      </section>

      <section className="decision-delta-grid">
        <section className="panel">
          <div className="panel-head">
            <h3>{t("decisionDelta.timeline")}</h3>
          </div>
          {items.length ? (
            <div className="decision-delta-list">
              {items.map((item) => (
                <DecisionDeltaRow
                  key={item.decision.id}
                  item={item}
                  active={item.decision.id === selectedId}
                  onSelect={() => setSelectedId(item.decision.id)}
                  t={t}
                />
              ))}
            </div>
          ) : (
            <EmptyState title={t("decisionDelta.noTimelineTitle")}>
              {t("decisionDelta.noTimelineBody")}
            </EmptyState>
          )}
        </section>

        <section className="panel decision-delta-detail">
          {detail.data && selectedItem ? (
            <DecisionDeltaDetailView
              detail={detail.data}
              selectedItem={selectedItem}
              reviewForm={reviewForm}
              setReviewForm={setReviewForm}
              selectedPrinciples={selectedPrinciples}
              setSelectedPrinciples={setSelectedPrinciples}
              selectedChecklist={selectedChecklist}
              setSelectedChecklist={setSelectedChecklist}
              onSubmitReview={submitReview}
              onAdopt={() => adoptCandidates.mutate()}
              snapshotHistoryLimit={snapshotHistoryLimit}
              savePending={saveReview.isPending}
              adoptPending={adoptCandidates.isPending}
              t={t}
            />
          ) : (
            <EmptyState title={t("decisionDelta.noSelectionTitle")}>
              {t("decisionDelta.noSelectionBody")}
            </EmptyState>
          )}
        </section>
      </section>
    </div>
  );
}

function DecisionDeltaRow({
  item,
  active,
  onSelect,
  t
}: {
  item: DecisionDeltaTimelineItem;
  active: boolean;
  onSelect: () => void;
  t: (key: TranslationKey, values?: Record<string, string | number>) => string;
}) {
  const snapshot = item.latest_snapshot;
  const stale = isDecisionDeltaStale(snapshot);
  const tone = decisionDeltaTone(snapshot);

  return (
    <button
      className={active ? "decision-delta-row active" : "decision-delta-row"}
      type="button"
      onClick={onSelect}
    >
      <div className="decision-delta-row-main">
        <strong>{item.decision.symbol ?? item.decision.action}</strong>
        <p>{item.decision.action} · {formatDate(item.decision.created_at)}</p>
      </div>
      <div className="decision-delta-row-meta">
        <span className={item.quantifiable ? "pill" : "pill warning"}>
          {item.quantifiable ? t("decisionDelta.quantified") : t("decisionDelta.unquantified")}
        </span>
        <span className={item.reviewed ? "pill" : "pill warning"}>
          {item.reviewed ? t("decisionDelta.reviewed") : t("decisionDelta.notReviewed")}
        </span>
        {stale ? <span className="pill warning">{t("common.stale")}</span> : null}
      </div>
      <strong className={`${tone}-text decision-delta-row-value`}>
        {snapshot ? formatDecisionDeltaMoney(snapshot.delta_value, "CNY") : "n/a"}
      </strong>
    </button>
  );
}

function DecisionDeltaDetailView({
  detail,
  selectedItem,
  reviewForm,
  setReviewForm,
  selectedPrinciples,
  setSelectedPrinciples,
  selectedChecklist,
  setSelectedChecklist,
  onSubmitReview,
  onAdopt,
  snapshotHistoryLimit,
  savePending,
  adoptPending,
  t
}: {
  detail: DecisionDeltaDetail;
  selectedItem: DecisionDeltaTimelineItem;
  reviewForm: ReviewForm;
  setReviewForm: (form: ReviewForm) => void;
  selectedPrinciples: string[];
  setSelectedPrinciples: (values: string[]) => void;
  selectedChecklist: string[];
  setSelectedChecklist: (values: string[]) => void;
  onSubmitReview: (event: FormEvent) => void;
  onAdopt: () => void;
  snapshotHistoryLimit: number;
  savePending: boolean;
  adoptPending: boolean;
  t: (key: TranslationKey, values?: Record<string, string | number>) => string;
}) {
  const { actual, baseline } = singleForkLegs(detail.legs);
  const comparisonKey = comparisonLabelKey(comparisonKindForAction(detail.decision.action));
  const snapshot = detail.latest_snapshot;
  const canAdopt = selectedPrinciples.length > 0 || selectedChecklist.length > 0;

  return (
    <>
      <div className="panel-head">
        <div>
          <h3>{detail.decision.symbol ?? t("decisionDelta.detail")}</h3>
          <p>{t(comparisonKey)}</p>
        </div>
        <span className={selectedItem.reviewed ? "pill" : "pill warning"}>
          {selectedItem.reviewed ? t("decisionDelta.reviewed") : t("decisionDelta.notReviewed")}
        </span>
      </div>

      <div className="decision-copy-grid">
        <div>
          <strong>{t("decisionDelta.rationale")}</strong>
          <p>{detail.decision.rationale}</p>
        </div>
        <div>
          <strong>{t("decisionDelta.expectedOutcome")}</strong>
          <p>{detail.decision.expected_outcome}</p>
        </div>
      </div>

      <div className="decision-fork">
        <LegCard title={t("decisionDelta.actual")} leg={actual} />
        <div className="decision-fork-vs">{t("decisionDelta.vs")}</div>
        <LegCard title={t("decisionDelta.baseline")} leg={baseline} />
      </div>

      <section className="decision-delta-snapshot">
        <div className="panel-head">
          <h3>{t("decisionDelta.latestSnapshot")}</h3>
        </div>
        <div className="stats-grid compact-stats-grid">
          <StatCard
            label={t("decisionDelta.actualValue")}
            value={snapshot ? money(snapshot.actual_value, "CNY") : "n/a"}
          />
          <StatCard
            label={t("decisionDelta.baselineValue")}
            value={snapshot ? money(snapshot.baseline_value, "CNY") : "n/a"}
          />
          <StatCard
            label={t("decisionDelta.tableDelta")}
            value={snapshot ? formatDecisionDeltaMoney(snapshot.delta_value, "CNY") : "n/a"}
            tone={decisionDeltaTone(snapshot)}
          />
          <StatCard
            label={t("decisionDelta.deltaPct")}
            value={formatDecisionDeltaPercent(snapshot?.delta_pct)}
            tone={decisionDeltaTone(snapshot)}
          />
        </div>
      </section>

      <section className="decision-delta-history">
        <div className="panel-head">
          <h3>
            {t("decisionDelta.snapshotHistoryLimited", {
              count: snapshotHistoryLimit
            })}
          </h3>
        </div>
        <div className="data-table-wrap">
          <table>
            <thead>
              <tr>
                <th>{t("decisionDelta.tableDate")}</th>
                <th>{t("decisionDelta.tableActual")}</th>
                <th>{t("decisionDelta.tableBaseline")}</th>
                <th>{t("decisionDelta.tableDelta")}</th>
                <th>{t("decisionDelta.tableStatus")}</th>
              </tr>
            </thead>
            <tbody>
              {detail.snapshots.map((item) => (
                <tr key={item.id}>
                  <td>{formatDate(item.created_at)}</td>
                  <td>{money(item.actual_value, "CNY")}</td>
                  <td>{money(item.baseline_value, "CNY")}</td>
                  <td className={`${decisionDeltaTone(item)}-text`}>
                    {formatDecisionDeltaMoney(item.delta_value, "CNY")}
                  </td>
                  <td>{isDecisionDeltaStale(item) ? t("common.stale") : t("decisionDelta.fresh")}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <form className="decision-delta-review" onSubmit={onSubmitReview}>
        <div className="panel-head">
          <h3>{t("decisionDelta.review")}</h3>
        </div>
        <label>
          <span>{t("decisionDelta.reviewNotes")}</span>
          <textarea
            value={reviewForm.notes}
            onChange={(event) => setReviewForm({ ...reviewForm, notes: event.target.value })}
            rows={3}
          />
        </label>
        <div className="review-form-grid">
          <ReviewTextArea
            label={t("decisionDelta.thesisEvidence")}
            value={reviewForm.thesis_evidence}
            onChange={(value) => setReviewForm({ ...reviewForm, thesis_evidence: value })}
          />
          <ReviewTextArea
            label={t("decisionDelta.disconfirmingEvidence")}
            value={reviewForm.disconfirming_evidence}
            onChange={(value) => setReviewForm({ ...reviewForm, disconfirming_evidence: value })}
          />
          <ReviewTextArea
            label={t("decisionDelta.lessons")}
            value={reviewForm.lessons}
            onChange={(value) => setReviewForm({ ...reviewForm, lessons: value })}
          />
          <ReviewTextArea
            label={t("decisionDelta.candidatePrinciples")}
            value={reviewForm.candidate_principles}
            onChange={(value) => setReviewForm({ ...reviewForm, candidate_principles: value })}
          />
          <ReviewTextArea
            label={t("decisionDelta.candidateChecklist")}
            value={reviewForm.candidate_checklist_items}
            onChange={(value) => setReviewForm({ ...reviewForm, candidate_checklist_items: value })}
          />
        </div>
        <button className="primary-button fit-button" type="submit" disabled={savePending}>
          <Save size={18} />
          {savePending ? t("decisionDelta.savingReview") : t("decisionDelta.saveReview")}
        </button>
      </form>

      <section className="decision-delta-adoption">
        <div className="panel-head">
          <h3>{t("decisionDelta.adoptSelected")}</h3>
        </div>
        <CandidateChecklist
          title={t("decisionDelta.candidatePrinciples")}
          values={detail.review?.candidate_principles ?? []}
          selected={selectedPrinciples}
          onChange={setSelectedPrinciples}
          emptyLabel={t("decisionDelta.noCandidates")}
        />
        <CandidateChecklist
          title={t("decisionDelta.candidateChecklist")}
          values={detail.review?.candidate_checklist_items ?? []}
          selected={selectedChecklist}
          onChange={setSelectedChecklist}
          emptyLabel={t("decisionDelta.noCandidates")}
        />
        <button
          className="primary-button fit-button"
          type="button"
          disabled={!canAdopt || adoptPending}
          onClick={onAdopt}
        >
          <Check size={18} />
          {adoptPending ? t("decisionDelta.adopting") : t("decisionDelta.adoptSelected")}
        </button>
      </section>
    </>
  );
}

function LegCard({ title, leg }: { title: string; leg: DecisionDeltaLeg | null }) {
  return (
    <div className="fork-leg">
      <span>{title}</span>
      <strong>{legTitle(leg)}</strong>
      <p>{legDescription(leg)}</p>
    </div>
  );
}

function ReviewTextArea({
  label,
  value,
  onChange
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <label>
      <span>{label}</span>
      <textarea value={value} onChange={(event) => onChange(event.target.value)} rows={3} />
    </label>
  );
}

function CandidateChecklist({
  title,
  values,
  selected,
  onChange,
  emptyLabel
}: {
  title: string;
  values: string[];
  selected: string[];
  onChange: (values: string[]) => void;
  emptyLabel: string;
}) {
  return (
    <div className="candidate-box">
      <strong>{title}</strong>
      {values.length ? (
        values.map((value) => (
          <label className="checkbox-row" key={value}>
            <input
              type="checkbox"
              checked={selected.includes(value)}
              onChange={() => onChange(toggleValue(selected, value))}
            />
            <span>{value}</span>
          </label>
        ))
      ) : (
        <p>{emptyLabel}</p>
      )}
    </div>
  );
}

function splitLines(value: string) {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function toggleValue(values: string[], value: string) {
  return values.includes(value)
    ? values.filter((item) => item !== value)
    : [...values, value];
}

function comparisonLabelKey(kind: ReturnType<typeof comparisonKindForAction>): TranslationKey {
  switch (kind) {
    case "asset_vs_cash":
      return "decisionDelta.assetVsCash";
    case "cash_vs_holding":
      return "decisionDelta.cashVsHolding";
    case "cash_vs_hypothetical":
      return "decisionDelta.cashVsHypothetical";
    default:
      return "decisionDelta.manualComparison";
  }
}

function legTitle(leg: DecisionDeltaLeg | null) {
  if (!leg) {
    return "n/a";
  }
  return leg.symbol ?? leg.baseline_type ?? "cash";
}

function legDescription(leg: DecisionDeltaLeg | null) {
  if (!leg) {
    return "n/a";
  }
  if (leg.symbol) {
    const quantity = leg.quantity == null ? "n/a" : number(leg.quantity);
    const price = leg.price == null ? "market" : money(leg.price, leg.currency);
    return `${quantity} × ${price}`;
  }
  return money(leg.notional ?? 0, leg.currency);
}

function money(value: number, currency: string) {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency,
    maximumFractionDigits: 2
  }).format(value);
}

function number(value: number) {
  return new Intl.NumberFormat("en-US", { maximumFractionDigits: 4 }).format(value);
}

function formatDate(value: string) {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return parsed.toLocaleDateString();
}
