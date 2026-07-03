import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Check,
  ClipboardPaste,
  Edit3,
  FileUp,
  ImageUp,
  RefreshCw,
  Save,
  Trash2,
  X
} from "lucide-react";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { AiWebSocketClient, type AiWsServerMessage } from "../api/aiWs";
import { api, type FilePayload, type ImagePayload } from "../api/client";
import { EmptyState } from "../components/EmptyState";
import { StatCard } from "../components/StatCard";
import { useI18n, type TranslationKey } from "../i18n";
import type {
  PortfolioDraftRow,
  PortfolioImageImportPreview,
  PortfolioImageImportTask,
  PortfolioImportMapping,
  PortfolioImportPreview,
  PortfolioPosition
} from "../types/domain";
import {
  canCommitDraftRows,
  draftRowsForCommit,
  emptyPortfolioDraftRow,
  draftRowHasWarnings,
  formatBaseMoney,
  formatMoney,
  marketGroupsForDisplay,
  percent,
  positionEditDraft,
  positionUpdatePayload,
  rowsWithDuplicateSymbolErrors,
  updateDraftRowField,
  type PortfolioDraftEditableField,
  type PositionEditDraft
} from "./portfolioRules";

type DraftTableRow = PortfolioDraftRow & {
  source_id?: string;
  source_label?: string;
};

type ImageImportTaskState = PortfolioImageImportTask & {
  payload: ImagePayload;
  source_label: string;
  started_at: number | null;
};

type FileImportMode = "append" | "replace";

export function PortfolioPage() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const positions = useQuery({ queryKey: ["positions"], queryFn: api.positions });
  const summary = useQuery({ queryKey: ["portfolio-summary"], queryFn: api.portfolioSummary });
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
  const aiWsRef = useRef<AiWebSocketClient | null>(null);
  const imageTasksRef = useRef<ImageImportTaskState[]>([]);
  const fileImportModeRef = useRef<FileImportMode>("replace");
  const fileImportSourceIdRef = useRef<string | null>(null);

  const marketGroups = useMemo(
    () => (summary.data ? marketGroupsForDisplay(summary.data) : []),
    [summary.data]
  );
  const chartData = useMemo(
    () =>
      marketGroups.map((group) => ({
        label: group.label,
        weight: Number.parseFloat(group.weightLabel)
      })),
    [marketGroups]
  );
  const draftHasRows = draftRows.length > 0;
  const draftCanCommit = canCommitDraftRows(draftRows);

  useEffect(() => {
    const client = new AiWebSocketClient();
    aiWsRef.current = client;
    const unsubscribe = client.onMessage(handleAiWsMessage);
    client.connect().catch((error) => {
      setDraftError(error instanceof Error ? error.message : String(error));
    });

    return () => {
      unsubscribe();
      client.close();
      aiWsRef.current = null;
    };
  }, []);

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

  const refreshPrices = useMutation({
    mutationFn: api.refreshPrices,
    onSuccess: () => {
      invalidatePortfolio(queryClient);
      queryClient.invalidateQueries({ queryKey: ["profile"] });
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

  const actionError =
    draftError ??
    previewPortfolioImport.error?.message ??
    commitDraft.error?.message ??
    refreshPrices.error?.message ??
    updatePosition.error?.message ??
    deletePosition.error?.message ??
    null;

  async function handleFile(file: File | null) {
    if (!file) {
      return;
    }
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

  async function handleClipboardImage() {
    const clipboard = navigator.clipboard as (Clipboard & { read?: () => Promise<ClipboardItem[]> }) | undefined;
    if (!clipboard?.read) {
      setDraftError(t("portfolio.clipboardUnsupported"));
      return;
    }

    try {
      const items = await clipboard.read();
      for (const item of items) {
        const imageType = item.types.find((type) => type.startsWith("image/"));
        if (imageType) {
          const blob = await item.getType(imageType);
          await handleImageFiles([new File([blob], `clipboard.${extensionForMime(imageType)}`, { type: imageType })]);
          return;
        }
      }
    } catch (error) {
      setDraftError(error instanceof Error ? error.message : t("portfolio.clipboardUnsupported"));
      return;
    }

    setDraftError(t("portfolio.noClipboardImage"));
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

  function mergeDraftRows(
    rows: PortfolioDraftRow[],
    sourceId: string,
    sourceLabel: string,
    mode: FileImportMode
  ) {
    const sourcedRows = rows.map((row) => ({
      ...row,
      source_id: sourceId,
      source_label: sourceLabel
    }));
    setDraftRows((current) => {
      const base = mode === "replace" ? [] : current.filter((row) => row.source_id !== sourceId);
      return rowsWithDuplicateSymbolErrors([...base, ...sourcedRows]) as DraftTableRow[];
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
    const client = aiWsRef.current;
    if (!client) {
      updateImageTaskFailure(task.id, "AI WebSocket is not connected");
      startQueuedImageImports();
      return;
    }

    client
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
      aiWsRef.current?.send({ type: "cancel", request_id: id }).catch(() => undefined);
    } else {
      updateImageTask(id, { status: "canceled", stage: "canceled" });
      startQueuedImageImports();
    }
  }

  function updateDraftRow(index: number, field: PortfolioDraftEditableField, value: string) {
    setDraftRows((current) =>
      rowsWithDuplicateSymbolErrors(
        current.map((row, rowIndex) => (rowIndex === index ? updateDraftRowField(row, field, value) : row))
      ) as DraftTableRow[]
    );
  }

  function removeDraftRow(index: number) {
    setDraftRows((current) => rowsWithDuplicateSymbolErrors(current.filter((_, rowIndex) => rowIndex !== index)) as DraftTableRow[]);
  }

  function clearDraft() {
    imageTasksRef.current.forEach((task) => {
      if (task.status === "running") {
        aiWsRef.current?.send({ type: "cancel", request_id: task.id }).catch(() => undefined);
      }
    });
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
          <button className="ghost-button" type="button" onClick={() => setImportOpen((open) => !open)}>
            <FileUp size={18} />
            {t("portfolio.importTools")}
          </button>
          <button className="primary-button" type="button" onClick={() => refreshPrices.mutate()}>
            <RefreshCw size={18} />
            {t("portfolio.refreshPrices")}
          </button>
        </div>
      </header>

      <section className="stats-grid">
        <StatCard label={t("portfolio.cnyTotal")} value={summary.data ? formatBaseMoney(summary.data) : "CN¥0.00"} />
        <StatCard
          label={t("portfolio.cnyPl")}
          value={formatMoney(summary.data?.total_unrealized_pnl_base ?? 0, "CNY")}
          tone={(summary.data?.total_unrealized_pnl_base ?? 0) >= 0 ? "positive" : "warning"}
        />
        <StatCard
          label={t("portfolio.marketValue")}
          value={`${marketGroups.length}`}
          detail={t("portfolio.marketGroups")}
        />
        <StatCard
          label={t("portfolio.stalePrices")}
          value={`${summary.data?.price_stale_count ?? 0}`}
          detail={t("portfolio.fxStaleCount", { count: summary.data?.fx_stale_count ?? 0 })}
          tone={(summary.data?.price_stale_count ?? 0) + (summary.data?.fx_stale_count ?? 0) > 0 ? "warning" : "neutral"}
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
            <button className="ghost-button" type="button" onClick={addManualDraftRow}>
              <Edit3 size={18} />
              {t("portfolio.addManualRow")}
            </button>
            <label className="file-button">
              <FileUp size={18} />
              {t("portfolio.chooseFile")}
              <input
                type="file"
                accept=".csv,.tsv,.xlsx"
                onChange={(event) => handleFile(event.target.files?.[0] ?? null)}
              />
            </label>
            <label className="file-button">
              <ImageUp size={18} />
              {t("portfolio.chooseImage")}
              <input
                type="file"
                accept="image/png,image/jpeg,image/jpg,image/webp"
                multiple
                onChange={(event) => handleImageFiles(event.target.files)}
              />
            </label>
            <button className="ghost-button" type="button" onClick={handleClipboardImage}>
              <ClipboardPaste size={18} />
              {t("portfolio.pasteImage")}
            </button>
            {draftSource ? <span className="pill">{draftSource}</span> : null}
          </div>

          {previewPortfolioImport.isPending || draftPortfolioImport.isPending ? (
            <div className="warning-box">{t("portfolio.preparingDraft")}</div>
          ) : null}
          <ImageImportTasks tasks={imageTasks} onCancel={cancelImageTask} />
          {actionError ? <div className="warning-box">{actionError}</div> : null}
          {draftWarnings.length ? <div className="warning-box">{draftWarnings.join(" ")}</div> : null}

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
                <RefreshCw size={18} />
                {t("portfolio.applyMapping")}
              </button>
            </div>
          ) : null}

          <DraftTable rows={draftRows} onChange={updateDraftRow} onRemove={removeDraftRow} />

          <div className="settings-actions">
            <button
              className="primary-button"
              type="button"
              disabled={!draftCanCommit || commitDraft.isPending}
              onClick={() => commitDraft.mutate({ rows: draftRowsForCommit(rowsWithDuplicateSymbolErrors(draftRows)) })}
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

      <section className="dashboard-grid">
        <section className="panel">
          <div className="panel-head">
            <h3>{t("portfolio.positions")}</h3>
          </div>
          {(positions.data ?? []).length ? (
            <div className="data-table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>{t("portfolio.tableSymbol")}</th>
                    <th>{t("portfolio.tableName")}</th>
                    <th>{t("portfolio.mapMarket")}</th>
                    <th>{t("portfolio.tableQty")}</th>
                    <th>{t("portfolio.tableAvgCost")}</th>
                    <th>{t("portfolio.tableMarketValue")}</th>
                    <th>{t("portfolio.tablePl")}</th>
                    <th>{t("portfolio.tableWeight")}</th>
                    <th>{t("portfolio.tableStatus")}</th>
                    <th>{t("portfolio.actions")}</th>
                  </tr>
                </thead>
                <tbody>
                  {(positions.data ?? []).map((position) => (
                    <tr key={position.symbol}>
                      <td>
                        <strong>{position.symbol}</strong>
                      </td>
                      <td>{position.name}</td>
                      <td>{position.market ?? "Other"}</td>
                      <td>{number(position.quantity)}</td>
                      <td>{formatMoney(position.average_cost, position.currency)}</td>
                      <td>{formatMoney(position.market_value, position.currency)}</td>
                      <td className={position.unrealized_pnl >= 0 ? "positive-text" : "warning-text"}>
                        {formatMoney(position.unrealized_pnl, position.currency)}
                      </td>
                      <td>{percent(position.weight)}</td>
                      <td>
                        <span className={position.price_stale ? "pill warning" : "pill"}>
                          {position.price_stale ? t("common.stale") : t("common.fresh")}
                        </span>
                      </td>
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

        <section className="panel">
          <div className="panel-head">
            <h3>{t("portfolio.marketAllocation")}</h3>
          </div>
          {marketGroups.length ? (
            <div className="market-stack">
              <ResponsiveContainer width="100%" height={220}>
                <BarChart data={chartData}>
                  <CartesianGrid strokeDasharray="3 3" vertical={false} />
                  <XAxis dataKey="label" />
                  <YAxis tickFormatter={(value) => `${value}%`} />
                  <Tooltip formatter={(value: number) => `${value.toFixed(1)}%`} />
                  <Bar dataKey="weight" fill="#2f6f73" radius={[4, 4, 0, 0]} />
                </BarChart>
              </ResponsiveContainer>
              <div className="legend-list">
                {marketGroups.map((group) => (
                  <div className="legend-row" key={group.label}>
                    <strong>{group.label}</strong>
                    <em>{group.nativeValue}</em>
                    <em>{group.weightLabel}</em>
                    {group.stale ? <span className="pill warning">{t("portfolio.fxStale")}</span> : null}
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <EmptyState title={t("portfolio.noExposureTitle")}>{t("portfolio.noExposureBody")}</EmptyState>
          )}
        </section>
      </section>

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
                <input
                  value={editing.draft[field.key]}
                  onChange={(event) => updateEditDraft(field.key, event.target.value)}
                />
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
  onChange,
  onRemove
}: {
  rows: DraftTableRow[];
  onChange: (index: number, field: PortfolioDraftEditableField, value: string) => void;
  onRemove: (index: number) => void;
}) {
  const { t } = useI18n();

  if (!rows.length) {
    return <EmptyState title={t("portfolio.emptyDraftTitle")}>{t("portfolio.emptyDraftBody")}</EmptyState>;
  }

  return (
    <div className="preview-table-wrap">
      <table className="draft-table">
        <thead>
          <tr>
            <th>{t("portfolio.source")}</th>
            {draftFields.map((field) => (
              <th key={field.key}>{t(field.labelKey)}</th>
            ))}
            <th>{t("portfolio.confidence")}</th>
            <th>{t("portfolio.rowIssues")}</th>
            <th>{t("portfolio.actions")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row, index) => (
            <tr key={`${row.symbol}-${index}`}>
              <td>{row.source_label ?? "-"}</td>
              {draftFields.map((field) => (
                <td key={field.key}>
                  <input
                    className="draft-input"
                    value={(row[field.key] as string | null | undefined) ?? ""}
                    onChange={(event) => onChange(index, field.key, event.target.value)}
                  />
                </td>
              ))}
              <td>
                <span className={draftRowHasWarnings(row) ? "pill warning" : "pill"}>{row.confidence}</span>
              </td>
              <td>{[...row.errors, ...row.warnings].join(" ") || "-"}</td>
              <td>
                <button className="icon-button danger" type="button" onClick={() => onRemove(index)}>
                  <Trash2 size={16} />
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
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

const emptyMapping: PortfolioImportMapping = {
  symbol: "",
  name: "",
  quantity: "",
  average_cost: "",
  currency: ""
};

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

const draftFields: Array<{ key: PortfolioDraftEditableField; labelKey: TranslationKey }> = [
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

function extensionForMime(mimeType: string) {
  if (mimeType === "image/jpeg" || mimeType === "image/jpg") {
    return "jpg";
  }
  if (mimeType === "image/webp") {
    return "webp";
  }
  return "png";
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

function invalidatePortfolio(queryClient: ReturnType<typeof useQueryClient>) {
  queryClient.invalidateQueries({ queryKey: ["positions"] });
  queryClient.invalidateQueries({ queryKey: ["portfolio-summary"] });
}
