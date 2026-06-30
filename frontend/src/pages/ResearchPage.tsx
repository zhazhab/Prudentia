import { FormEvent, useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Bot, Check, FileText, Search, Sparkles } from "lucide-react";
import { api } from "../api/client";
import { EmptyState } from "../components/EmptyState";
import { useI18n, type TranslationKey } from "../i18n";
import type { DistillResearchRequest, ResearchRecord, ResearchRecordKind } from "../types/domain";

const initialDistillForm = {
  title: "",
  source_type: "",
  source_title: "",
  source_author: "",
  symbol: "",
  source_content: ""
};

const initialStockForm = {
  symbol: ""
};

type DistillForm = typeof initialDistillForm;
type StockForm = typeof initialStockForm;

const kindLabelKeys: Record<ResearchRecordKind, TranslationKey> = {
  distillation: "research.kindDistillation",
  stock_snapshot: "research.kindStockSnapshot",
  portfolio_review: "research.kindPortfolioReview"
};

export function ResearchPage() {
  const { languageTag, t } = useI18n();
  const queryClient = useQueryClient();
  const [distillForm, setDistillForm] = useState<DistillForm>(initialDistillForm);
  const [stockForm, setStockForm] = useState<StockForm>(initialStockForm);
  const [filters, setFilters] = useState({ kind: "", symbol: "", q: "" });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedPrinciples, setSelectedPrinciples] = useState<string[]>([]);
  const [selectedChecklist, setSelectedChecklist] = useState<string[]>([]);

  const records = useQuery({
    queryKey: ["research-records", filters],
    queryFn: () => api.researchRecords(filters)
  });
  const record = useQuery({
    queryKey: ["research-record", selectedId],
    queryFn: () => api.researchRecord(selectedId ?? ""),
    enabled: Boolean(selectedId)
  });

  useEffect(() => {
    if (!records.data) {
      return;
    }
    if (!records.data.length) {
      setSelectedId(null);
      return;
    }
    if (!selectedId || !records.data.some((item) => item.id === selectedId)) {
      setSelectedId(records.data[0].id);
    }
  }, [records.data, selectedId]);

  useEffect(() => {
    setSelectedPrinciples([]);
    setSelectedChecklist([]);
  }, [record.data?.id]);

  const distillResearch = useMutation({
    mutationFn: (payload: DistillResearchRequest) => api.distillResearch(payload, languageTag),
    onSuccess: (created) => {
      queryClient.invalidateQueries({ queryKey: ["research-records"] });
      queryClient.setQueryData(["research-record", created.id], created);
      setSelectedId(created.id);
      setDistillForm(initialDistillForm);
    }
  });

  const stockSnapshot = useMutation({
    mutationFn: () =>
      api.stockSnapshot({ symbol: stockForm.symbol.trim().toUpperCase() }, languageTag),
    onSuccess: (created) => {
      queryClient.invalidateQueries({ queryKey: ["research-records"] });
      queryClient.setQueryData(["research-record", created.id], created);
      setSelectedId(created.id);
      setStockForm(initialStockForm);
    }
  });

  const portfolioReview = useMutation({
    mutationFn: () => api.portfolioReview(languageTag),
    onSuccess: (created) => {
      queryClient.invalidateQueries({ queryKey: ["research-records"] });
      queryClient.setQueryData(["research-record", created.id], created);
      setSelectedId(created.id);
    }
  });

  const adoptCandidates = useMutation({
    mutationFn: () =>
      api.adoptResearchCandidates(
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

  const selectedIdInList = Boolean(
    selectedId && records.data?.some((item) => item.id === selectedId)
  );
  const selectedRecord = selectedIdInList ? record.data : null;
  const hasSelectedCandidates = selectedPrinciples.length > 0 || selectedChecklist.length > 0;
  const actionError = useMemo(
    () =>
      distillResearch.error?.message ??
      stockSnapshot.error?.message ??
      portfolioReview.error?.message ??
      adoptCandidates.error?.message,
    [adoptCandidates.error, distillResearch.error, portfolioReview.error, stockSnapshot.error]
  );

  function submitDistill(event: FormEvent) {
    event.preventDefault();
    distillResearch.mutate({
      title: distillForm.title.trim(),
      source_type: emptyToNull(distillForm.source_type),
      source_title: emptyToNull(distillForm.source_title),
      source_author: emptyToNull(distillForm.source_author),
      source_content: distillForm.source_content.trim(),
      symbol: emptyToNull(distillForm.symbol)
    });
  }

  function submitStockSnapshot(event: FormEvent) {
    event.preventDefault();
    stockSnapshot.mutate();
  }

  return (
    <div className="page-stack">
      <header className="page-header">
        <div>
          <span className="eyebrow">{t("research.eyebrow")}</span>
          <h2>{t("research.title")}</h2>
        </div>
      </header>

      <section className="research-actions">
        <form className="panel research-action" onSubmit={submitDistill}>
          <div className="panel-head">
            <h3>{t("research.distill")}</h3>
            <FileText size={18} />
          </div>
          <label>
            <span>{t("research.recordTitle")}</span>
            <input
              value={distillForm.title}
              onChange={(event) => setDistillForm({ ...distillForm, title: event.target.value })}
              required
            />
          </label>
          <div className="research-form-pair">
            <label>
              <span>{t("research.sourceType")}</span>
              <input
                value={distillForm.source_type}
                onChange={(event) =>
                  setDistillForm({ ...distillForm, source_type: event.target.value })
                }
              />
            </label>
            <label>
              <span>{t("research.symbol")}</span>
              <input
                value={distillForm.symbol}
                onChange={(event) => setDistillForm({ ...distillForm, symbol: event.target.value })}
                placeholder="AAPL"
              />
            </label>
          </div>
          <label>
            <span>{t("research.sourceTitle")}</span>
            <input
              value={distillForm.source_title}
              onChange={(event) =>
                setDistillForm({ ...distillForm, source_title: event.target.value })
              }
            />
          </label>
          <label>
            <span>{t("research.sourceAuthor")}</span>
            <input
              value={distillForm.source_author}
              onChange={(event) =>
                setDistillForm({ ...distillForm, source_author: event.target.value })
              }
            />
          </label>
          <label>
            <span>{t("research.sourceContent")}</span>
            <textarea
              value={distillForm.source_content}
              onChange={(event) =>
                setDistillForm({ ...distillForm, source_content: event.target.value })
              }
              rows={5}
              required
            />
          </label>
          <button className="primary-button" type="submit" disabled={distillResearch.isPending}>
            <Sparkles size={18} />
            {t("research.run")}
          </button>
        </form>

        <form className="panel research-action" onSubmit={submitStockSnapshot}>
          <div className="panel-head">
            <h3>{t("research.stockSnapshot")}</h3>
            <Search size={18} />
          </div>
          <label>
            <span>{t("research.symbol")}</span>
            <input
              value={stockForm.symbol}
              onChange={(event) => setStockForm({ symbol: event.target.value })}
              placeholder="MSFT"
              required
            />
          </label>
          <button className="primary-button" type="submit" disabled={stockSnapshot.isPending}>
            <Bot size={18} />
            {t("research.run")}
          </button>
        </form>

        <section className="panel research-action">
          <div className="panel-head">
            <h3>{t("research.portfolioReview")}</h3>
            <Bot size={18} />
          </div>
          <button
            className="primary-button"
            type="button"
            onClick={() => portfolioReview.mutate()}
            disabled={portfolioReview.isPending}
          >
            <Sparkles size={18} />
            {t("research.run")}
          </button>
        </section>
      </section>

      {actionError ? <div className="warning-box">{actionError}</div> : null}

      <section className="research-grid">
        <section className="panel">
          <div className="panel-head">
            <h3>{t("research.records")}</h3>
          </div>
          <div className="research-filters">
            <label>
              <span>{t("research.filters")}</span>
              <select
                value={filters.kind}
                onChange={(event) => setFilters({ ...filters, kind: event.target.value })}
              >
                <option value="">{t("research.kindAll")}</option>
                <option value="distillation">{t("research.kindDistillation")}</option>
                <option value="stock_snapshot">{t("research.kindStockSnapshot")}</option>
                <option value="portfolio_review">{t("research.kindPortfolioReview")}</option>
              </select>
            </label>
            <label>
              <span>{t("research.symbol")}</span>
              <input
                value={filters.symbol}
                onChange={(event) => setFilters({ ...filters, symbol: event.target.value })}
                placeholder="BRK.B"
              />
            </label>
            <label>
              <span>{t("research.search")}</span>
              <input
                value={filters.q}
                onChange={(event) => setFilters({ ...filters, q: event.target.value })}
              />
            </label>
          </div>

          {records.isPending ? (
            <EmptyState title={t("research.loading")}>{t("research.loadingRecords")}</EmptyState>
          ) : records.isError ? (
            <EmptyState title={t("research.errorTitle")}>{records.error.message}</EmptyState>
          ) : (records.data ?? []).length ? (
            <div className="memo-list">
              {(records.data ?? []).map((item) => (
                <ResearchRecordRow
                  key={item.id}
                  record={item}
                  active={selectedId === item.id}
                  label={t(kindLabelKeys[item.kind])}
                  onSelect={() => setSelectedId(item.id)}
                />
              ))}
            </div>
          ) : (
            <EmptyState title={t("research.noRecordsTitle")}>
              {t("research.noRecordsBody")}
            </EmptyState>
          )}
        </section>

        <section className="panel">
          {selectedId && !selectedIdInList && records.isFetching ? (
            <EmptyState title={t("research.loading")}>{t("research.loadingSelection")}</EmptyState>
          ) : selectedId && selectedIdInList && record.isPending ? (
            <EmptyState title={t("research.loading")}>{t("research.loadingSelection")}</EmptyState>
          ) : selectedId && selectedIdInList && record.isError ? (
            <EmptyState title={t("research.errorTitle")}>{record.error.message}</EmptyState>
          ) : selectedRecord ? (
            <div className="research-detail">
              <div className="panel-head">
                <div>
                  <h3>{selectedRecord.title}</h3>
                  <p>{recordMeta(selectedRecord, t(kindLabelKeys[selectedRecord.kind]))}</p>
                </div>
                <span className="pill">{selectedRecord.symbol ?? t(kindLabelKeys[selectedRecord.kind])}</span>
              </div>

              <MemoBlock label={t("research.summary")} value={selectedRecord.summary} />
              <ArrayBlock label={t("research.insights")} values={selectedRecord.insights} />
              <ArrayBlock label={t("research.risks")} values={selectedRecord.risks} />
              <ArrayBlock label={t("research.checklist")} values={selectedRecord.checklist} />

              <CandidateBlock
                label={t("research.candidatePrinciples")}
                values={selectedRecord.candidate_principles}
                selected={selectedPrinciples}
                onToggle={(value) => setSelectedPrinciples(toggleItem(selectedPrinciples, value))}
              />
              <CandidateBlock
                label={t("research.candidateChecklist")}
                values={selectedRecord.candidate_checklist_items}
                selected={selectedChecklist}
                onToggle={(value) => setSelectedChecklist(toggleItem(selectedChecklist, value))}
              />

              <button
                className="primary-button"
                type="button"
                onClick={() => adoptCandidates.mutate()}
                disabled={!hasSelectedCandidates || adoptCandidates.isPending}
              >
                <Check size={18} />
                {t("research.adoptSelected")}
              </button>
            </div>
          ) : (
            <EmptyState title={t("research.noSelectionTitle")}>
              {t("research.noSelectionBody")}
            </EmptyState>
          )}
        </section>
      </section>
    </div>
  );
}

function ResearchRecordRow({
  record,
  active,
  label,
  onSelect
}: {
  record: ResearchRecord;
  active: boolean;
  label: string;
  onSelect: () => void;
}) {
  return (
    <button className={active ? "memo-row active" : "memo-row"} type="button" onClick={onSelect}>
      <div>
        <strong>{record.title}</strong>
        <p>{record.symbol ?? record.source_title ?? record.source_type ?? label}</p>
      </div>
      <span className="pill">{label}</span>
    </button>
  );
}

function MemoBlock({ label, value }: { label: string; value: string }) {
  return (
    <div className="memo-block">
      <strong>{label}</strong>
      <p>{value || "-"}</p>
    </div>
  );
}

function ArrayBlock({ label, values }: { label: string; values: string[] }) {
  return (
    <div className="checklist-box">
      <strong>{label}</strong>
      {values.length ? values.map((item) => <span key={item}>{item}</span>) : <span>-</span>}
    </div>
  );
}

function CandidateBlock({
  label,
  values,
  selected,
  onToggle
}: {
  label: string;
  values: string[];
  selected: string[];
  onToggle: (value: string) => void;
}) {
  return (
    <div className="checklist-box">
      <strong>{label}</strong>
      {values.length ? (
        values.map((item) => (
          <label className="checkbox-row" key={item}>
            <input
              type="checkbox"
              checked={selected.includes(item)}
              onChange={() => onToggle(item)}
            />
            <span>{item}</span>
          </label>
        ))
      ) : (
        <span>-</span>
      )}
    </div>
  );
}

function toggleItem(items: string[], item: string) {
  return items.includes(item) ? items.filter((value) => value !== item) : [...items, item];
}

function emptyToNull(value: string) {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

function recordMeta(record: ResearchRecord, kindLabel: string) {
  return [kindLabel, record.source_title, record.source_author].filter(Boolean).join(" / ");
}
