import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  Check,
  Edit3,
  FileUp,
  Plus,
  RefreshCw,
  Save,
  SearchCheck,
  SlidersHorizontal,
  Trash2,
  X
} from "lucide-react";
import {
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis
} from "recharts";
import type { AiWsServerMessage } from "../api/aiWs";
import { api, type FilePayload, type ImagePayload } from "../api/client";
import { useAiWebSocket } from "../api/useAiWebSocket";
import { EmptyState } from "../components/EmptyState";
import { StatCard } from "../components/StatCard";
import { useI18n, type TranslationKey } from "../i18n";
import type {
  PortfolioDraftRow,
  PortfolioImageImportPreview,
  PortfolioImageImportTask,
  PortfolioImportMapping,
  PortfolioImportPreview,
  PortfolioPerformancePeriod,
  PortfolioPerformanceResponse,
  PortfolioPosition
} from "../types/domain";
import {
  canCommitDraftRows,
  currencyOptionsForValue,
  defaultPositionSortRule,
  draftEditableDisplayFields,
  draftRowsForCommit,
  emptyPortfolioDraftRow,
  ensureDraftRowClientIds,
  formatBaseMoney,
  formatMoney,
  formatReturnPercent,
  marketOptionsForValue,
  mergeDuplicateDraftRowsBySymbol,
  nextPositionSortRule,
  normalizePositionSortRule,
  performanceChartRows,
  performanceChartYAxisDomain,
  percent,
  positionTableSortableFields,
  positionTableDisplayFields,
  portfolioDashboardPanelIds,
  portfolioImportFileKind,
  portfolioIssueLabels,
  positionEditDraft,
  positionUpdatePayload,
  sortPositions,
  updateDraftRowField,
  type BenchmarkComparisonMetric,
  type PortfolioDraftEditableField,
  type PositionEditDraft,
  type PositionSortField,
  type PositionSortRule,
  type PositionTableDisplayField
} from "./portfolioRules";

type DraftTableRow = PortfolioDraftRow & {
  client_row_id: string;
  source_id?: string;
  source_label?: string;
};

type ImageImportTaskState = PortfolioImageImportTask & {
  payload: ImagePayload;
  source_label: string;
  started_at: number | null;
};

type FileImportMode = "append" | "replace";
type PerformanceView = "amount" | "percent";

const positionSortStorageKey = "prudentia.portfolio.positionSort";

function loadPositionSortRule(): PositionSortRule {
  if (typeof window === "undefined") {
    return defaultPositionSortRule;
  }

  try {
    const rawValue = window.localStorage.getItem(positionSortStorageKey);
    return normalizePositionSortRule(rawValue ? JSON.parse(rawValue) : null);
  } catch {
    return defaultPositionSortRule;
  }
}

function savePositionSortRule(sortRule: PositionSortRule) {
  if (typeof window === "undefined") {
    return;
  }

  try {
    window.localStorage.setItem(positionSortStorageKey, JSON.stringify(normalizePositionSortRule(sortRule)));
  } catch {
    // Local storage can be unavailable in restricted browser contexts.
  }
}

export function PortfolioPage() {
  const { locale, t } = useI18n();
  const queryClient = useQueryClient();
  const summary = useQuery({ queryKey: ["portfolio-summary"], queryFn: api.portfolioSummary });
  const [performancePeriod, setPerformancePeriod] = useState<PortfolioPerformancePeriod>("month");
  const [performanceView, setPerformanceView] = useState<PerformanceView>("percent");
  const [benchmarkMetric, setBenchmarkMetric] = useState<BenchmarkComparisonMetric>("cumulative");
  const [positionSort, setPositionSort] = useState<PositionSortRule>(loadPositionSortRule);
  const positions = useQuery({
    queryKey: ["positions", performancePeriod],
    queryFn: () => api.positions(performancePeriod)
  });
  const performance = useQuery({
    queryKey: ["portfolio-performance", performancePeriod],
    queryFn: () => api.portfolioPerformance(performancePeriod)
  });
  const [importOpen, setImportOpen] = useState(false);
  const [filePayload, setFilePayload] = useState<FilePayload | null>(null);
  const [preview, setPreview] = useState<PortfolioImportPreview | null>(null);
  const [mapping, setMapping] = useState<PortfolioImportMapping | null>(null);
  const [draftRows, setDraftRows] = useState<DraftTableRow[]>([]);
  const [draftWarnings, setDraftWarnings] = useState<string[]>([]);
  const [draftSource, setDraftSource] = useState<string | null>(null);
  const [draftError, setDraftError] = useState<string | null>(null);
  const [editing, setEditing] = useState<{ symbol: string; draft: PositionEditDraft } | null>(null);
  const [imageTasks, setImageTasks] = useState<ImageImportTaskState[]>([]);
  const { session: aiWs } = useAiWebSocket();
  const imageTasksRef = useRef<ImageImportTaskState[]>([]);
  const fileImportModeRef = useRef<FileImportMode>("replace");
  const fileImportSourceIdRef = useRef<string | null>(null);
  const draftRowIdRef = useRef(0);

  const performanceChartData = useMemo(
    () => performanceChartRows(performance.data, benchmarkMetric),
    [performance.data, benchmarkMetric]
  );
  const performanceChartDomain = useMemo(
    () => performanceChartYAxisDomain(performanceChartData),
    [performanceChartData]
  );
  const showPerformanceDots = performanceChartData.length < 2;
  const performanceMetric = performance.data?.portfolio;
  const performanceAmount = performanceMetric?.profit_loss_base ?? null;
  const performanceReturn = performanceMetric?.return_pct ?? null;
  const performanceAnnualized = performanceMetric?.annualized_return_pct ?? null;
  const performanceNetCashFlow = performanceMetric?.net_cash_flow_base ?? 0;
  const performanceSimpleReturn = performanceMetric?.simple_return_pct ?? null;
  const performanceValue =
    performanceView === "amount"
      ? performanceAmount === null
        ? "—"
        : formatMoney(performanceAmount, performance.data?.base_currency ?? "CNY")
      : formatOptionalPercent(performanceReturn);
  const performanceDetail =
    performanceView === "amount"
      ? t("portfolio.performanceTwrAmountDetail", {
          value: formatOptionalPercent(performanceReturn),
          flow: formatSignedMoney(performanceNetCashFlow, performance.data?.base_currency ?? "CNY"),
          simple: formatOptionalPercent(performanceSimpleReturn)
        })
      : t("portfolio.performanceTwrPercentDetail", {
          value:
            performanceAmount === null
              ? "—"
              : formatMoney(performanceAmount, performance.data?.base_currency ?? "CNY"),
          flow: formatSignedMoney(performanceNetCashFlow, performance.data?.base_currency ?? "CNY"),
          simple: formatOptionalPercent(performanceSimpleReturn)
        });
  const checkedDraftRows = useMemo(
    () => mergeDuplicateDraftRowsBySymbol(draftRows) as DraftTableRow[],
    [draftRows]
  );
  const sortedPositions = useMemo(
    () => sortPositions(positions.data ?? [], positionSort),
    [positions.data, positionSort]
  );
  const draftHasRows = draftRows.length > 0;
  const draftCanCommit = canCommitDraftRows(checkedDraftRows);

  useEffect(() => {
    return aiWs.onMessage(handleAiWsMessage);
  }, [aiWs]);

  useEffect(() => {
    return () => {
      cancelRunningImageTasks();
    };
  }, [aiWs]);

  useEffect(() => {
    savePositionSortRule(positionSort);
  }, [positionSort]);

  const previewPortfolioImport = useMutation({
    mutationFn: api.previewPortfolioImport,
    onSuccess: (result) => {
      setPreview(result);
      setMapping(result.suggested_mapping);
      mergeDraftRows(
        result.draft_rows,
        fileImportSourceIdRef.current ?? makeRequestId(),
        "file",
        fileImportModeRef.current
      );
      setDraftWarnings(result.validation_errors);
      setDraftSource("file");
      setDraftError(null);
      setImportOpen(true);
    }
  });

  const draftPortfolioImport = useMutation({
    mutationFn: api.draftPortfolioImport,
    onSuccess: (result) => {
      mergeDraftRows(
        result.draft_rows,
        fileImportSourceIdRef.current ?? makeRequestId(),
        result.source,
        fileImportModeRef.current
      );
      setDraftWarnings(result.warnings);
      setDraftSource(result.source);
      setDraftError(null);
    }
  });

  const commitDraft = useMutation({
    mutationFn: api.commitPortfolioDraft,
    onSuccess: () => {
      invalidatePortfolio(queryClient);
      clearDraft();
    }
  });

  const resolveDraftSymbols = useMutation({
    mutationFn: api.resolvePortfolioDraftSymbols,
    onSuccess: (result) => {
      setDraftRows((current) =>
        ensureDraftRowsHaveIds(
          mergeDuplicateDraftRowsBySymbol(
            result.draft_rows.map((row, index) => ({
              ...row,
              client_row_id: current[index]?.client_row_id,
              source_id: current[index]?.source_id,
              source_label: current[index]?.source_label
            }))
          )
        )
      );
      setDraftError(t("portfolio.symbolResolveDone", { count: result.resolved_count }));
    }
  });

  const updatePosition = useMutation({
    mutationFn: ({ symbol, payload }: { symbol: string; payload: ReturnType<typeof positionUpdatePayload> }) =>
      api.updatePosition(symbol, payload),
    onSuccess: () => {
      invalidatePortfolio(queryClient);
      setEditing(null);
    }
  });

  const deletePosition = useMutation({
    mutationFn: api.deletePosition,
    onSuccess: () => invalidatePortfolio(queryClient)
  });

  const refreshPrices = useMutation({
    mutationFn: api.refreshPortfolioPrices,
    onSuccess: () => invalidatePortfolio(queryClient)
  });

  const actionError =
    draftError ??
    previewPortfolioImport.error?.message ??
    commitDraft.error?.message ??
    resolveDraftSymbols.error?.message ??
    updatePosition.error?.message ??
    deletePosition.error?.message ??
    refreshPrices.error?.message ??
    null;

  async function handleFile(file: File | null) {
    if (!file) {
      return;
    }
    setDraftError(null);
    const payload = await fileToPayload(file);
    const mode =
      draftRowsForCommit(draftRows).length > 0 && window.confirm(t("portfolio.appendFileConfirm"))
        ? "append"
        : "replace";
    const sourceId = `file:${makeRequestId()}`;
    fileImportModeRef.current = mode;
    fileImportSourceIdRef.current = sourceId;
    setFilePayload(payload);
    previewPortfolioImport.mutate(payload);
  }

  async function handleImportFiles(files: FileList | File[] | null | undefined) {
    const selectedFiles = Array.from(files ?? []);
    if (!selectedFiles.length) {
      return;
    }

    const tabularFiles = selectedFiles.filter((file) => portfolioImportFileKind(file) === "tabular");
    const imageFiles = selectedFiles.filter((file) => portfolioImportFileKind(file) === "image");
    const unsupportedFiles = selectedFiles.filter((file) => portfolioImportFileKind(file) === "unsupported");

    if (unsupportedFiles.length) {
      setDraftError(t("portfolio.unsupportedImportFile", { name: unsupportedFiles[0].name }));
      return;
    }
    if (tabularFiles.length && imageFiles.length) {
      setDraftError(t("portfolio.mixedImportFileTypes"));
      return;
    }
    if (tabularFiles.length > 1) {
      setDraftError(t("portfolio.oneTabularFileOnly"));
      return;
    }
    if (tabularFiles.length === 1) {
      await handleFile(tabularFiles[0]);
      return;
    }

    await handleImageFiles(imageFiles);
  }

  function applyMapping() {
    if (!filePayload || !mapping) {
      return;
    }
    draftPortfolioImport.mutate({ ...filePayload, mapping });
  }

  async function handleImageFiles(files: FileList | File[] | null | undefined) {
    if (!files?.length) {
      return;
    }
    setDraftError(null);
    setPreview(null);
    setMapping(null);
    setFilePayload(null);
    setImportOpen(true);

    const tasks = await Promise.all(
      Array.from(files).map(async (file) => {
        const id = `image:${makeRequestId()}`;
        return {
          id,
          file_name: file.name,
          status: "queued" as const,
          stage: null,
          elapsed_ms: 0,
          recognized_rows: 0,
          error: null,
          payload: await imageToPayload(file),
          source_label: file.name,
          started_at: null
        };
      })
    );
    updateImageTasks((current) => [...current, ...tasks]);
    startQueuedImageImports();
  }

  function updateMapping(field: keyof PortfolioImportMapping, value: string) {
    setMapping((current) => ({
      ...(current ?? emptyMapping),
      [field]: value || null
    }));
  }

  function addManualDraftRow() {
    mergeDraftRows([emptyPortfolioDraftRow()], `manual:${makeRequestId()}`, t("portfolio.manualEntry"), "append");
  }

  function openImportTools() {
    setImportOpen(true);
  }

  function toggleImportTools() {
    if (importOpen) {
      setImportOpen(false);
    } else {
      openImportTools();
    }
  }

  function mergeDraftRows(
    rows: PortfolioDraftRow[],
    sourceId: string,
    sourceLabel: string,
    mode: FileImportMode
  ) {
    const sourcedRows = rows.map((row) => ({
      ...row,
      client_row_id: makeDraftRowClientId(),
      source_id: sourceId,
      source_label: sourceLabel
    }));
    setDraftRows((current) => {
      const base = mode === "replace" ? [] : current.filter((row) => row.source_id !== sourceId);
      return mergeDuplicateDraftRowsBySymbol([...base, ...sourcedRows]) as DraftTableRow[];
    });
  }

  function updateImageTasks(updater: (tasks: ImageImportTaskState[]) => ImageImportTaskState[]) {
    const next = updater(imageTasksRef.current);
    imageTasksRef.current = next;
    setImageTasks(next);
  }

  function startQueuedImageImports() {
    const tasks = imageTasksRef.current;
    const openSlots = Math.max(0, 2 - tasks.filter((task) => task.status === "running").length);
    const queued = tasks.filter((task) => task.status === "queued").slice(0, openSlots);
    queued.forEach(startImageImportTask);
  }

  function startImageImportTask(task: ImageImportTaskState) {
    const startedAt = Date.now();
    updateImageTasks((current) =>
      current.map((item) =>
        item.id === task.id ? { ...item, status: "running", stage: "queued", started_at: startedAt } : item
      )
    );
    aiWs
      .send({
        type: "portfolio_image_import.start",
        request_id: task.id,
        payload: task.payload
      })
      .catch((error) => {
        updateImageTaskFailure(task.id, error instanceof Error ? error.message : String(error));
        startQueuedImageImports();
      });
  }

  function handleAiWsMessage(message: AiWsServerMessage) {
    if (message.type === "accepted") {
      updateImageTask(message.request_id, { stage: "accepted" });
      return;
    }

    if (message.type === "progress") {
      updateImageTask(message.request_id, { stage: message.stage });
      return;
    }

    if (message.type === "failed") {
      updateImageTaskFailure(message.request_id, message.error);
      startQueuedImageImports();
      return;
    }

    if (message.type === "canceled") {
      updateImageTask(message.request_id, { status: "canceled", stage: "canceled" });
      startQueuedImageImports();
      return;
    }

    if (message.type === "completed" && message.artifact_type === "portfolio_image_import.preview") {
      const task = imageTasksRef.current.find((item) => item.id === message.request_id);
      const previewResult = message.data as PortfolioImageImportPreview;
      mergeDraftRows(previewResult.draft_rows, message.request_id, task?.source_label ?? "screenshot", "append");
      setDraftWarnings((current) => [...current, ...previewResult.warnings]);
      setDraftSource(previewResult.source);
      updateImageTask(message.request_id, {
        status: "completed",
        stage: "completed",
        recognized_rows: previewResult.draft_rows.length
      });
      startQueuedImageImports();
    }
  }

  function updateImageTask(id: string, patch: Partial<ImageImportTaskState>) {
    updateImageTasks((current) =>
      current.map((task) =>
        task.id === id
          ? {
              ...task,
              ...patch,
              elapsed_ms: task.started_at ? Date.now() - task.started_at : task.elapsed_ms
            }
          : task
      )
    );
  }

  function updateImageTaskFailure(id: string, error: string) {
    updateImageTask(id, { status: "failed", stage: "failed", error });
  }

  function cancelImageTask(id: string) {
    const task = imageTasksRef.current.find((item) => item.id === id);
    if (task?.status === "running") {
      aiWs.send({ type: "cancel", request_id: id }).catch(() => undefined);
    } else {
      updateImageTask(id, { status: "canceled", stage: "canceled" });
      startQueuedImageImports();
    }
  }

  function updateDraftRow(index: number, field: PortfolioDraftEditableField, value: string) {
    setDraftRows((current) =>
      mergeDuplicateDraftRowsBySymbol(
        current.map((row, rowIndex) => {
          if (rowIndex !== index) {
            return row;
          }
          return {
            ...row,
            ...updateDraftRowField(row, field, value),
            client_row_id: row.client_row_id
          };
        })
      ) as DraftTableRow[]
    );
  }

  function removeDraftRow(index: number) {
    setDraftRows((current) => mergeDuplicateDraftRowsBySymbol(current.filter((_, rowIndex) => rowIndex !== index)) as DraftTableRow[]);
  }

  function cancelRunningImageTasks() {
    imageTasksRef.current.forEach((task) => {
      if (task.status === "running") {
        aiWs.send({ type: "cancel", request_id: task.id }).catch(() => undefined);
      }
    });
  }

  function clearDraft() {
    cancelRunningImageTasks();
    setDraftRows([]);
    setDraftWarnings([]);
    setDraftSource(null);
    setDraftError(null);
    updateImageTasks(() => []);
    setPreview(null);
    setMapping(null);
    setFilePayload(null);
    previewPortfolioImport.reset();
    draftPortfolioImport.reset();
  }

  function ensureDraftRowsHaveIds(rows: Array<PortfolioDraftRow & Partial<DraftTableRow>>): DraftTableRow[] {
    return ensureDraftRowClientIds(rows, makeDraftRowClientId) as DraftTableRow[];
  }

  function makeDraftRowClientId() {
    draftRowIdRef.current += 1;
    return `draft-row:${draftRowIdRef.current}`;
  }

  function startEditing(position: PortfolioPosition) {
    setEditing({ symbol: position.symbol, draft: positionEditDraft(position) });
  }

  function updateEditDraft(field: keyof PositionEditDraft, value: string) {
    setEditing((current) =>
      current ? { ...current, draft: { ...current.draft, [field]: value } } : current
    );
  }

  return (
    <div className="page-stack">
      <header className="page-header">
        <div>
          <span className="eyebrow">{t("portfolio.eyebrow")}</span>
          <h2>{t("portfolio.title")}</h2>
        </div>
        <div className="import-actions">
          <button className="ghost-button" type="button" onClick={toggleImportTools}>
            <FileUp size={18} />
            {t("portfolio.importTools")}
          </button>
        </div>
      </header>

      <section className="panel performance-panel">
        <div className="performance-toolbar">
          <div>
            <h3>{t("portfolio.performance")}</h3>
            {performance.data?.partial_period && performance.data.start_date ? (
              <p>{t("portfolio.partialPeriod", { date: shortDate(performance.data.start_date) })}</p>
            ) : (
              <p>{t("portfolio.performanceSnapshotBasis")}</p>
            )}
          </div>
          <div className="performance-controls">
            <div className="segmented-control" aria-label={t("portfolio.performancePeriod")}>
              {performancePeriodOptions.map((option) => (
                <button
                  key={option.value}
                  className={performancePeriod === option.value ? "active" : ""}
                  type="button"
                  onClick={() => setPerformancePeriod(option.value)}
                >
                  {t(option.labelKey)}
                </button>
              ))}
            </div>
            <div className="segmented-control" aria-label={t("portfolio.performanceView")}>
              {performanceViewOptions.map((option) => (
                <button
                  key={option.value}
                  className={performanceView === option.value ? "active" : ""}
                  type="button"
                  onClick={() => setPerformanceView(option.value)}
                >
                  {t(option.labelKey)}
                </button>
              ))}
            </div>
          </div>
        </div>
      </section>

      <section className="stats-grid">
        <StatCard
          label={t("portfolio.periodReturn")}
          value={performance.isLoading ? t("portfolio.loadingPerformance") : performanceValue}
          detail={performanceDetail}
          tone={performanceAmount === null ? "neutral" : performanceAmount >= 0 ? "positive" : "warning"}
        />
        <StatCard
          label={t("portfolio.annualizedReturn")}
          value={formatOptionalPercent(performanceAnnualized)}
          detail={t("portfolio.annualizedReturnDetail")}
          tone={performanceAnnualized === null ? "neutral" : performanceAnnualized >= 0 ? "positive" : "warning"}
        />
        <StatCard label={t("portfolio.cnyTotal")} value={summary.data ? formatBaseMoney(summary.data) : "CNY 0.00"} />
        <StatCard
          label={t("portfolio.cnyPl")}
          value={formatMoney(summary.data?.total_unrealized_pnl_base ?? 0, "CNY")}
          tone={(summary.data?.total_unrealized_pnl_base ?? 0) >= 0 ? "positive" : "warning"}
        />
      </section>

      {importOpen ? (
        <section className="panel import-panel">
          <div className="panel-head">
            <div>
              <h3>{t("portfolio.importTools")}</h3>
              <p>{t("portfolio.importToolsBody")}</p>
            </div>
            {draftHasRows ? (
              <button className="ghost-button icon-text-button" type="button" onClick={clearDraft}>
                <X size={18} />
                {t("portfolio.clearDraft")}
              </button>
            ) : null}
          </div>

          <div className="import-source-row">
            <label className="file-button">
              <FileUp size={18} />
              {t("portfolio.addFile")}
              <input
                type="file"
                accept={portfolioImportFileAccept}
                multiple
                onChange={(event) => {
                  void handleImportFiles(event.target.files);
                  event.currentTarget.value = "";
                }}
              />
            </label>
            {draftHasRows ? (
              <button
                className="ghost-button"
                type="button"
                disabled={resolveDraftSymbols.isPending}
                onClick={() => resolveDraftSymbols.mutate({ rows: checkedDraftRows })}
              >
                <SearchCheck size={18} />
                {t("portfolio.resolveDraftSymbols")}
              </button>
            ) : null}
            {draftSource ? <span className="pill">{draftSource}</span> : null}
          </div>

          {previewPortfolioImport.isPending || draftPortfolioImport.isPending || resolveDraftSymbols.isPending ? (
            <div className="warning-box">{t("portfolio.preparingDraft")}</div>
          ) : null}
          <ImageImportTasks tasks={imageTasks} onCancel={cancelImageTask} />
          {actionError ? <div className="warning-box">{actionError}</div> : null}
          {draftWarnings.length ? (
            <div className="warning-box">{portfolioIssueLabels(draftWarnings, locale).join(" ")}</div>
          ) : null}

          {preview ? (
            <div className="import-grid">
              <div className="mapping-grid">
                {mappingFields.map((field) => (
                  <label key={field.key}>
                    <span>{t(field.labelKey)}</span>
                    <select
                      value={(mapping?.[field.key] as string | null | undefined) ?? ""}
                      onChange={(event) => updateMapping(field.key, event.target.value)}
                    >
                      <option value="">{t("portfolio.selectColumn")}</option>
                      {preview.headers.map((header) => (
                        <option key={header} value={header}>
                          {header}
                        </option>
                      ))}
                    </select>
                  </label>
                ))}
              </div>
              <button className="ghost-button fit-button" type="button" onClick={applyMapping}>
                <SlidersHorizontal size={18} />
                {t("portfolio.applyMapping")}
              </button>
            </div>
          ) : null}

          <DraftTable rows={checkedDraftRows} onAdd={addManualDraftRow} onChange={updateDraftRow} onRemove={removeDraftRow} />

          <div className="settings-actions">
            <button
              className="primary-button"
              type="button"
              disabled={!draftCanCommit || commitDraft.isPending}
              onClick={() => commitDraft.mutate({ rows: draftRowsForCommit(checkedDraftRows) })}
            >
              <Check size={18} />
              {t("portfolio.commitDraft")}
            </button>
            {!draftCanCommit && draftHasRows ? (
              <span className="field-help">{t("portfolio.fixDraftErrors")}</span>
            ) : null}
          </div>
        </section>
      ) : null}

      <section className="panel performance-chart-panel">
        <div className="panel-head">
          <div>
            <h3>{t("portfolio.benchmarkComparison")}</h3>
            <p>{t("portfolio.benchmarkProxyNote")}</p>
          </div>
          <div className="segmented-control" aria-label={t("portfolio.benchmarkMetric")}>
            {benchmarkMetricOptions.map((option) => (
              <button
                key={option.value}
                className={benchmarkMetric === option.value ? "active" : ""}
                type="button"
                onClick={() => setBenchmarkMetric(option.value)}
              >
                {t(option.labelKey)}
              </button>
            ))}
          </div>
        </div>
        {performanceChartData.length ? (
          <div className="performance-chart-stack">
            <ResponsiveContainer width="100%" height={260}>
              <LineChart data={performanceChartData}>
                <CartesianGrid strokeDasharray="3 3" vertical={false} />
                <XAxis dataKey="label" />
                <YAxis domain={performanceChartDomain} tickFormatter={(value) => formatChartTick(Number(value), benchmarkMetric)} />
                <Tooltip formatter={(value: number) => formatChartValue(value, benchmarkMetric)} />
                <Legend />
                {benchmarkMetric !== "excess" ? (
                  <Line
                    type="monotone"
                    dataKey="portfolio"
                    name={t("portfolio.performancePortfolio")}
                    stroke="#2f6f73"
                    strokeWidth={2.5}
                    dot={showPerformanceDots ? { r: 4 } : false}
                    activeDot={{ r: 5 }}
                    connectNulls
                  />
                ) : null}
                {benchmarkVisuals.map((benchmark) => (
                  <Line
                    key={benchmark.key}
                    type="monotone"
                    dataKey={benchmark.key}
                    name={benchmarkLineLabel(benchmarkMetric, benchmark.labelKey, t)}
                    stroke={benchmark.color}
                    strokeWidth={2}
                    dot={showPerformanceDots ? { r: 4 } : false}
                    activeDot={{ r: 5 }}
                    connectNulls
                  />
                ))}
              </LineChart>
            </ResponsiveContainer>
            {showPerformanceDots ? <p className="chart-note">{t("portfolio.singleSnapshotChartNote")}</p> : null}
            <div className="benchmark-status-list">
              {(performance.data?.benchmarks ?? []).map((benchmark) => (
                <span className={benchmark.available && !benchmark.stale ? "pill" : "pill warning"} key={benchmark.key}>
                  {benchmarkLabel(benchmark.key, benchmark.label, t)} · {benchmark.symbol} ·{" "}
                  {benchmarkStatusValue(benchmark, performance.data, benchmarkMetric, t)}
                </span>
              ))}
            </div>
          </div>
        ) : (
          <EmptyState title={t("portfolio.noPerformanceTitle")}>{t("portfolio.noPerformanceBody")}</EmptyState>
        )}
      </section>

      {portfolioDashboardPanelIds.includes("positions") ? (
        <section className="panel">
          <div className="panel-head">
            <h3>{t("portfolio.positions")}</h3>
            <button
              className="icon-button"
              type="button"
              aria-label={t("portfolio.refreshPrices")}
              title={t("portfolio.refreshPrices")}
              disabled={refreshPrices.isPending}
              onClick={() => refreshPrices.mutate()}
            >
              <RefreshCw size={18} />
            </button>
          </div>
          {sortedPositions.length ? (
            <div className="data-table-wrap">
              <table>
                <thead>
                  <tr>
                    {positionTableDisplayFields.map((field) => {
                      const label = t(positionTableFieldLabels[field]);
                      return (
                        <th key={field} aria-sort={positionHeaderAriaSort(positionSort, field)}>
                          {isPositionSortableField(field) ? (
                            <button
                              className="table-sort-button"
                              type="button"
                              onClick={() => setPositionSort((current) => nextPositionSortRule(current, field))}
                            >
                              <span>{label}</span>
                              {positionSortIcon(positionSort, field)}
                            </button>
                          ) : (
                            label
                          )}
                        </th>
                      );
                    })}
                    <th>{t("portfolio.actions")}</th>
                  </tr>
                </thead>
                <tbody>
                  {sortedPositions.map((position) => (
                    <tr key={position.symbol}>
                      {positionTableDisplayFields.map((field) => (
                        <td key={field} className={positionTableCellClass(position, field)}>
                          {positionTableCell(position, field)}
                        </td>
                      ))}
                      <td>
                        <div className="row-actions">
                          <button className="icon-button" type="button" onClick={() => startEditing(position)}>
                            <Edit3 size={16} />
                          </button>
                          <button
                            className="icon-button danger"
                            type="button"
                            onClick={() => deletePosition.mutate(position.symbol)}
                          >
                            <Trash2 size={16} />
                          </button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <EmptyState title={t("portfolio.noPositionsTitle")}>{t("portfolio.noPositionsBody")}</EmptyState>
          )}
        </section>
      ) : null}

      {editing ? (
        <section className="panel edit-position-panel">
          <div className="panel-head">
            <h3>{t("portfolio.editPosition")}</h3>
            <button className="ghost-button" type="button" onClick={() => setEditing(null)}>
              <X size={18} />
              {t("common.cancel")}
            </button>
          </div>
          <div className="mapping-grid">
            {editFields.map((field) => (
              <label key={field.key}>
                <span>{t(field.labelKey)}</span>
                {field.key === "currency" || field.key === "market" ? (
                  <select
                    value={editing.draft[field.key]}
                    onChange={(event) => updateEditDraft(field.key, event.target.value)}
                  >
                    <option value=""></option>
                    {optionsForPortfolioField(field.key, editing.draft[field.key]).map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                ) : (
                  <input
                    value={editing.draft[field.key]}
                    onChange={(event) => updateEditDraft(field.key, event.target.value)}
                  />
                )}
              </label>
            ))}
          </div>
          <button
            className="primary-button fit-button"
            type="button"
            onClick={() =>
              updatePosition.mutate({
                symbol: editing.symbol,
                payload: positionUpdatePayload(editing.draft)
              })
            }
          >
            <Save size={18} />
            {t("portfolio.savePosition")}
          </button>
        </section>
      ) : null}
    </div>
  );
}

function DraftTable({
  rows,
  onAdd,
  onChange,
  onRemove
}: {
  rows: DraftTableRow[];
  onAdd: () => void;
  onChange: (index: number, field: PortfolioDraftEditableField, value: string) => void;
  onRemove: (index: number) => void;
}) {
  const { t } = useI18n();
  const addRowButton = (
    <button className="icon-button" type="button" aria-label={t("portfolio.addManualRow")} title={t("portfolio.addManualRow")} onClick={onAdd}>
      <Plus size={16} />
    </button>
  );

  if (!rows.length) {
    return (
      <EmptyState title={t("portfolio.emptyDraftTitle")} action={addRowButton}>
        {t("portfolio.emptyDraftBody")}
      </EmptyState>
    );
  }

  return (
    <div className="draft-table-shell">
      <div className="draft-table-toolbar">{addRowButton}</div>
      <div className="preview-table-wrap">
        <table className="draft-table">
          <thead>
            <tr>
              {draftFields.map((field) => (
                <th key={field.key}>{t(field.labelKey)}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, index) => (
              <tr key={row.client_row_id}>
                {draftFields.map((field) => (
                  <td key={field.key}>
                    {field.key === "currency" || field.key === "market" ? (
                      <select
                        aria-invalid={draftFieldHasError(row, field.key)}
                        className={`draft-input${draftFieldHasError(row, field.key) ? " error" : ""}`}
                        value={(row[field.key] as string | null | undefined) ?? ""}
                        onChange={(event) => onChange(index, field.key, event.target.value)}
                      >
                        <option value=""></option>
                        {optionsForPortfolioField(
                          field.key,
                          row[field.key] as string | null | undefined
                        ).map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                    ) : (
                      <div className={field.key === "symbol" ? "draft-symbol-cell" : undefined}>
                        <input
                          aria-invalid={draftFieldHasError(row, field.key)}
                          className={`draft-input${draftFieldHasError(row, field.key) ? " error" : ""}`}
                          value={(row[field.key] as string | null | undefined) ?? ""}
                          onChange={(event) => onChange(index, field.key, event.target.value)}
                        />
                        {field.key === "symbol" ? (
                          <button className="icon-button danger" type="button" onClick={() => onRemove(index)}>
                            <Trash2 size={16} />
                          </button>
                        ) : null}
                      </div>
                    )}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function draftFieldHasError(row: PortfolioDraftRow, field: (typeof draftEditableDisplayFields)[number]) {
  return row.errors.some((error) => {
    const normalized = error.toLocaleLowerCase();
    if (field === "symbol") {
      return normalized.includes("symbol");
    }
    if (field === "name") {
      return normalized.includes("name");
    }
    if (field === "quantity") {
      return normalized.includes("quantity");
    }
    if (field === "average_cost") {
      return normalized.includes("average_cost");
    }
    if (field === "currency") {
      return normalized.includes("currency");
    }
    if (field === "market") {
      return normalized === "market is required";
    }
    return false;
  });
}

function optionsForPortfolioField(
  field: PortfolioDraftEditableField | keyof PositionEditDraft,
  value: string | null | undefined
) {
  if (field === "currency") {
    return currencyOptionsForValue(value);
  }
  if (field === "market") {
    return marketOptionsForValue(value);
  }
  return [];
}

function ImageImportTasks({
  tasks,
  onCancel
}: {
  tasks: ImageImportTaskState[];
  onCancel: (id: string) => void;
}) {
  const { t } = useI18n();

  if (!tasks.length) {
    return null;
  }

  return (
    <div className="image-task-list">
      {tasks.map((task) => (
        <div className="image-task-row" key={task.id}>
          <div>
            <strong>{task.file_name}</strong>
            <span>{t(stageLabelKey(task.stage ?? task.status))}</span>
            {task.error ? <em>{task.error}</em> : null}
          </div>
          <span className={task.status === "failed" ? "pill warning" : "pill"}>
            {task.status === "completed"
              ? t("portfolio.imageRowsRecognized", { count: task.recognized_rows })
              : t(statusLabelKey(task.status))}
          </span>
          {task.status === "running" || task.status === "queued" ? (
            <button className="ghost-button fit-button" type="button" onClick={() => onCancel(task.id)}>
              <X size={16} />
              {t("common.cancel")}
            </button>
          ) : null}
        </div>
      ))}
    </div>
  );
}

function positionTableCell(position: PortfolioPosition, field: PositionTableDisplayField) {
  switch (field) {
    case "symbol":
      return <strong>{position.symbol}</strong>;
    case "name":
      return position.name;
    case "market":
      return position.market ?? "Other";
    case "quantity":
      return number(position.quantity);
    case "average_cost":
      return formatMoney(position.average_cost, position.currency);
    case "market_value":
      return formatMoney(position.market_value, position.currency);
    case "unrealized_pnl":
      return formatMoney(position.unrealized_pnl, position.currency);
    case "unrealized_pnl_pct":
      return formatOptionalPercent(position.unrealized_pnl_pct);
    case "period_return_pct":
      return formatOptionalPercent(position.period_return_pct);
    case "weight":
      return percent(position.weight);
  }
}

function isPositionSortableField(field: PositionTableDisplayField): field is PositionSortField {
  return positionTableSortableFields.includes(field as PositionSortField);
}

function positionHeaderAriaSort(
  sortRule: PositionSortRule,
  field: PositionTableDisplayField
): "ascending" | "descending" | undefined {
  if (!isPositionSortableField(field) || sortRule.field !== field) {
    return undefined;
  }
  return sortRule.direction === "desc" ? "descending" : "ascending";
}

function positionSortIcon(sortRule: PositionSortRule, field: PositionSortField) {
  if (sortRule.field !== field) {
    return <ArrowUpDown aria-hidden="true" size={13} />;
  }
  return sortRule.direction === "desc" ? (
    <ArrowDown aria-hidden="true" size={13} />
  ) : (
    <ArrowUp aria-hidden="true" size={13} />
  );
}

function positionTableCellClass(position: PortfolioPosition, field: PositionTableDisplayField) {
  if (field === "unrealized_pnl" || field === "unrealized_pnl_pct") {
    return position.unrealized_pnl >= 0 ? "positive-text" : "warning-text";
  }
  if (field === "period_return_pct") {
    if (position.period_return_pct === null || position.period_return_pct === undefined) {
      return undefined;
    }
    return position.period_return_pct >= 0 ? "positive-text" : "warning-text";
  }
  return undefined;
}

const emptyMapping: PortfolioImportMapping = {
  symbol: "",
  name: "",
  quantity: "",
  average_cost: "",
  currency: ""
};

const portfolioImportFileAccept = ".csv,.tsv,.xlsx,image/png,image/jpeg,image/jpg,image/webp";

const positionTableFieldLabels: Record<PositionTableDisplayField, TranslationKey> = {
  symbol: "portfolio.tableSymbol",
  name: "portfolio.tableName",
  market: "portfolio.mapMarket",
  quantity: "portfolio.tableQty",
  average_cost: "portfolio.tableAvgCost",
  market_value: "portfolio.tableMarketValue",
  unrealized_pnl: "portfolio.tablePl",
  unrealized_pnl_pct: "portfolio.tablePlPct",
  period_return_pct: "portfolio.tablePeriodReturnPct",
  weight: "portfolio.tableWeight"
};

const performancePeriodOptions: Array<{ value: PortfolioPerformancePeriod; labelKey: TranslationKey }> = [
  { value: "month", labelKey: "portfolio.periodMonth" },
  { value: "year", labelKey: "portfolio.periodYear" },
  { value: "since_inception", labelKey: "portfolio.periodSinceInception" }
];

const performanceViewOptions: Array<{ value: PerformanceView; labelKey: TranslationKey }> = [
  { value: "amount", labelKey: "portfolio.viewAmount" },
  { value: "percent", labelKey: "portfolio.viewPercent" }
];

const benchmarkMetricOptions: Array<{ value: BenchmarkComparisonMetric; labelKey: TranslationKey }> = [
  { value: "cumulative", labelKey: "portfolio.benchmarkMetricCumulative" },
  { value: "annualized", labelKey: "portfolio.benchmarkMetricAnnualized" },
  { value: "excess", labelKey: "portfolio.benchmarkMetricExcess" }
];

const benchmarkVisuals: Array<{ key: string; labelKey: TranslationKey; color: string }> = [
  { key: "sp500", labelKey: "portfolio.benchmarkSp500", color: "#4c6fbf" },
  { key: "hang_seng", labelKey: "portfolio.benchmarkHangSeng", color: "#b46b40" },
  { key: "sse", labelKey: "portfolio.benchmarkSse", color: "#8561a8" }
];

const mappingFields: Array<{ key: keyof PortfolioImportMapping; labelKey: TranslationKey }> = [
  { key: "symbol", labelKey: "portfolio.mapSymbol" },
  { key: "name", labelKey: "portfolio.mapName" },
  { key: "quantity", labelKey: "portfolio.mapQuantity" },
  { key: "average_cost", labelKey: "portfolio.mapAverageCost" },
  { key: "currency", labelKey: "portfolio.mapCurrency" },
  { key: "market", labelKey: "portfolio.mapMarket" },
  { key: "account", labelKey: "portfolio.mapAccount" },
  { key: "sector", labelKey: "portfolio.mapSector" },
  { key: "imported_market_value", labelKey: "portfolio.mapImportedMarketValue" },
  { key: "notes", labelKey: "portfolio.mapNotes" }
];

const draftFieldLabels: Record<(typeof draftEditableDisplayFields)[number], TranslationKey> = {
  symbol: "portfolio.mapSymbol",
  name: "portfolio.mapName",
  quantity: "portfolio.mapQuantity",
  average_cost: "portfolio.mapAverageCost",
  currency: "portfolio.mapCurrency",
  market: "portfolio.mapMarket"
};

const draftFields: Array<{ key: (typeof draftEditableDisplayFields)[number]; labelKey: TranslationKey }> =
  draftEditableDisplayFields.map((key) => ({ key, labelKey: draftFieldLabels[key] }));

const editFields: Array<{ key: keyof PositionEditDraft; labelKey: TranslationKey }> = [
  { key: "name", labelKey: "portfolio.mapName" },
  { key: "quantity", labelKey: "portfolio.mapQuantity" },
  { key: "average_cost", labelKey: "portfolio.mapAverageCost" },
  { key: "currency", labelKey: "portfolio.mapCurrency" },
  { key: "market", labelKey: "portfolio.mapMarket" },
  { key: "account", labelKey: "portfolio.mapAccount" },
  { key: "sector", labelKey: "portfolio.mapSector" },
  { key: "imported_market_value", labelKey: "portfolio.mapImportedMarketValue" },
  { key: "notes", labelKey: "portfolio.mapNotes" }
];

function statusLabelKey(status: ImageImportTaskState["status"]): TranslationKey {
  switch (status) {
    case "queued":
      return "portfolio.imageStatusQueued";
    case "running":
      return "portfolio.imageStatusRunning";
    case "completed":
      return "portfolio.imageStatusCompleted";
    case "failed":
      return "portfolio.imageStatusFailed";
    case "canceled":
      return "portfolio.imageStatusCanceled";
  }
}

function stageLabelKey(stage: string): TranslationKey {
  switch (stage) {
    case "accepted":
      return "portfolio.imageStageAccepted";
    case "validating_image":
      return "portfolio.imageStageValidating";
    case "writing_temp_image":
      return "portfolio.imageStageUploading";
    case "recognizing_image":
      return "portfolio.imageStageRecognizing";
    case "normalizing_rows":
      return "portfolio.imageStageNormalizing";
    case "resolving_symbols":
      return "portfolio.imageStageResolvingSymbols";
    case "completed":
      return "portfolio.imageStatusCompleted";
    case "failed":
      return "portfolio.imageStatusFailed";
    case "canceled":
      return "portfolio.imageStatusCanceled";
    default:
      return "portfolio.imageStatusQueued";
  }
}

async function fileToPayload(file: File): Promise<FilePayload> {
  if (file.name.endsWith(".xlsx")) {
    return {
      file_name: file.name,
      content: await fileToBase64(file),
      content_encoding: "base64"
    };
  }

  return {
    file_name: file.name,
    content: await file.text()
  };
}

async function imageToPayload(file: File): Promise<ImagePayload> {
  return {
    file_name: file.name,
    content: await fileToBase64(file),
    content_encoding: "base64",
    mime_type: file.type || mimeFromName(file.name)
  };
}

async function fileToBase64(file: File) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  bytes.forEach((byte) => {
    binary += String.fromCharCode(byte);
  });
  return btoa(binary);
}

function mimeFromName(fileName: string) {
  const lowerName = fileName.toLowerCase();
  if (lowerName.endsWith(".jpg") || lowerName.endsWith(".jpeg")) {
    return "image/jpeg";
  }
  if (lowerName.endsWith(".webp")) {
    return "image/webp";
  }
  return "image/png";
}

function makeRequestId() {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function number(value: number) {
  return new Intl.NumberFormat("en-US", { maximumFractionDigits: 4 }).format(value);
}

function formatOptionalPercent(value: number | null | undefined) {
  return value === null || value === undefined ? "—" : formatReturnPercent(value);
}

function formatSignedMoney(value: number, currency: string) {
  if (Math.abs(value) < Number.EPSILON) {
    return formatMoney(0, currency);
  }
  return `${value > 0 ? "+" : ""}${formatMoney(value, currency)}`;
}

function formatPercentPoints(value: number | null | undefined) {
  return value === null || value === undefined ? "—" : `${(value * 100).toFixed(2)} pp`;
}

function formatChartTick(value: number, metric: BenchmarkComparisonMetric) {
  return metric === "excess" ? `${value}pp` : `${value}%`;
}

function formatChartValue(value: number, metric: BenchmarkComparisonMetric) {
  return metric === "excess" ? `${value.toFixed(2)} pp` : `${value.toFixed(2)}%`;
}

function shortDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value.slice(0, 10);
  }
  return new Intl.DateTimeFormat("zh-CN", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit"
  })
    .format(date)
    .replaceAll("/", "-");
}

function benchmarkLabel(
  key: string,
  fallback: string,
  t: (key: TranslationKey, values?: Record<string, string | number>) => string
) {
  const visual = benchmarkVisuals.find((item) => item.key === key);
  return visual ? t(visual.labelKey) : fallback;
}

function benchmarkLineLabel(
  metric: BenchmarkComparisonMetric,
  labelKey: TranslationKey,
  t: (key: TranslationKey, values?: Record<string, string | number>) => string
) {
  const label = t(labelKey);
  return metric === "excess" ? t("portfolio.benchmarkExcessLabel", { benchmark: label }) : label;
}

function benchmarkStatusValue(
  benchmark: PortfolioPerformanceResponse["benchmarks"][number],
  performance: PortfolioPerformanceResponse | undefined,
  metric: BenchmarkComparisonMetric,
  t: (key: TranslationKey, values?: Record<string, string | number>) => string
) {
  if (!benchmark.available) {
    return t("portfolio.benchmarkUnavailable");
  }
  if (metric === "annualized") {
    return formatOptionalPercent(benchmark.annualized_return_pct);
  }
  if (metric === "excess") {
    const portfolioReturn = performance?.portfolio.return_pct;
    const benchmarkReturn = benchmark.return_pct;
    if (portfolioReturn === null || portfolioReturn === undefined || benchmarkReturn === null || benchmarkReturn === undefined) {
      return "—";
    }
    return formatPercentPoints(portfolioReturn - benchmarkReturn);
  }
  return formatOptionalPercent(benchmark.return_pct);
}

function invalidatePortfolio(queryClient: ReturnType<typeof useQueryClient>) {
  queryClient.invalidateQueries({ queryKey: ["positions"] });
  queryClient.invalidateQueries({ queryKey: ["portfolio-summary"] });
  queryClient.invalidateQueries({ queryKey: ["portfolio-performance"] });
}
