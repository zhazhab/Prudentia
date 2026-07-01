import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, ClipboardPaste, FileUp, ImageUp, RefreshCw, X } from "lucide-react";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { api, type FilePayload, type ImagePayload } from "../api/client";
import { EmptyState } from "../components/EmptyState";
import { StatCard } from "../components/StatCard";
import { useI18n, type TranslationKey } from "../i18n";
import type { PortfolioImageDraftRow, PortfolioImportMapping, PortfolioImportPreview } from "../types/domain";

export function PortfolioPage() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const positions = useQuery({ queryKey: ["positions"], queryFn: api.positions });
  const summary = useQuery({ queryKey: ["portfolio-summary"], queryFn: api.portfolioSummary });
  const [filePayload, setFilePayload] = useState<FilePayload | null>(null);
  const [preview, setPreview] = useState<PortfolioImportPreview | null>(null);
  const [mapping, setMapping] = useState<PortfolioImportMapping | null>(null);
  const [imageRows, setImageRows] = useState<PortfolioImageDraftRow[]>([]);
  const [imageWarnings, setImageWarnings] = useState<string[]>([]);
  const [imageError, setImageError] = useState<string | null>(null);

  const previewImport = useMutation({
    mutationFn: api.previewPortfolioImport,
    onSuccess: (result) => {
      setPreview(result);
      setMapping(result.suggested_mapping);
    }
  });

  const commitImport = useMutation({
    mutationFn: api.commitPortfolioImport,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["positions"] });
      queryClient.invalidateQueries({ queryKey: ["portfolio-summary"] });
    }
  });

  const previewImageImport = useMutation({
    mutationFn: api.previewPortfolioImageImport,
    onSuccess: (result) => {
      setImageRows(result.rows);
      setImageWarnings(result.warnings);
      setImageError(null);
    }
  });

  const refreshPrices = useMutation({
    mutationFn: api.refreshPrices,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["positions"] });
      queryClient.invalidateQueries({ queryKey: ["portfolio-summary"] });
      queryClient.invalidateQueries({ queryKey: ["profile"] });
    }
  });

  const sectorData = useMemo(
    () => summary.data?.sectors.map((slice) => ({ ...slice, weightLabel: percent(slice.weight) })) ?? [],
    [summary.data]
  );
  const imageRecognitionError = imageError ?? previewImageImport.error?.message ?? null;

  async function handleFile(file: File | null) {
    if (!file) {
      return;
    }
    const payload = await fileToPayload(file);
    setFilePayload(payload);
    previewImport.mutate(payload);
  }

  async function handleImageFile(file: File | null) {
    if (!file) {
      return;
    }
    setImageError(null);
    const payload = await imageToPayload(file);
    previewImageImport.mutate(payload);
  }

  async function handleClipboardImage() {
    const clipboard = navigator.clipboard as (Clipboard & { read?: () => Promise<ClipboardItem[]> }) | undefined;
    if (!clipboard?.read) {
      setImageError(t("portfolio.clipboardUnsupported"));
      return;
    }

    try {
      const items = await clipboard.read();
      for (const item of items) {
        const imageType = item.types.find((type) => type.startsWith("image/"));
        if (imageType) {
          const blob = await item.getType(imageType);
          await handleImageFile(new File([blob], `clipboard.${extensionForMime(imageType)}`, { type: imageType }));
          return;
        }
      }
    } catch (error) {
      setImageError(error instanceof Error ? error.message : t("portfolio.clipboardUnsupported"));
      return;
    }

    setImageError(t("portfolio.noClipboardImage"));
  }

  function updateMapping(field: keyof PortfolioImportMapping, value: string) {
    setMapping((current) => ({
      ...(current ?? emptyMapping),
      [field]: value || null
    }));
  }

  function updateImageRow(index: number, field: ImageDraftEditableField, value: string) {
    setImageRows((current) =>
      current.map((row, rowIndex) =>
        rowIndex === index
          ? {
              ...row,
              [field]: optionalImageDraftFields.includes(field) ? value || null : value
            }
          : row
      )
    );
  }

  function clearImagePreview() {
    setImageRows([]);
    setImageWarnings([]);
    setImageError(null);
    previewImageImport.reset();
  }

  function commit() {
    if (!filePayload || !mapping) {
      return;
    }
    commitImport.mutate({ ...filePayload, mapping });
  }

  return (
    <div className="page-stack">
      <header className="page-header">
        <div>
          <span className="eyebrow">{t("portfolio.eyebrow")}</span>
          <h2>{t("portfolio.title")}</h2>
        </div>
        <button className="primary-button" type="button" onClick={() => refreshPrices.mutate()}>
          <RefreshCw size={18} />
          {t("portfolio.refreshPrices")}
        </button>
      </header>

      <section className="stats-grid">
        <StatCard label={t("portfolio.marketValue")} value={currency(summary.data?.total_market_value ?? 0)} />
        <StatCard label={t("portfolio.costBasis")} value={currency(summary.data?.total_cost ?? 0)} />
        <StatCard
          label={t("dashboard.unrealizedPl")}
          value={currency(summary.data?.total_unrealized_pnl ?? 0)}
          tone={(summary.data?.total_unrealized_pnl ?? 0) >= 0 ? "positive" : "warning"}
        />
        <StatCard
          label={t("portfolio.stalePrices")}
          value={`${summary.data?.price_stale_count ?? 0}`}
          detail={t("common.positions", { count: summary.data?.positions_count ?? 0 })}
        />
      </section>

      <section className="panel import-panel">
        <div className="panel-head">
          <h3>{t("portfolio.importPositions")}</h3>
          <label className="file-button">
            <FileUp size={18} />
            {t("portfolio.chooseFile")}
            <input
              type="file"
              accept=".csv,.tsv,.xlsx"
              onChange={(event) => handleFile(event.target.files?.[0] ?? null)}
            />
          </label>
        </div>

        <div className="image-import-area">
          <div className="image-import-head">
            <div>
              <h4>{t("portfolio.recognizeImage")}</h4>
              <p>{t("portfolio.imagePrivacy")}</p>
            </div>
            <div className="import-actions">
              <label className="file-button">
                <ImageUp size={18} />
                {t("portfolio.chooseImage")}
                <input
                  type="file"
                  accept="image/png,image/jpeg,image/jpg,image/webp"
                  onChange={(event) => handleImageFile(event.target.files?.[0] ?? null)}
                />
              </label>
              <button className="ghost-button" type="button" onClick={handleClipboardImage}>
                <ClipboardPaste size={18} />
                {t("portfolio.pasteImage")}
              </button>
              {imageRows.length || imageWarnings.length || imageRecognitionError ? (
                <button className="ghost-button icon-text-button" type="button" onClick={clearImagePreview}>
                  <X size={18} />
                  {t("portfolio.clearImagePreview")}
                </button>
              ) : null}
            </div>
          </div>

          {previewImageImport.isPending ? <div className="warning-box">{t("portfolio.recognizingImage")}</div> : null}
          {imageRecognitionError ? <div className="warning-box">{imageRecognitionError}</div> : null}
          {imageWarnings.length ? <div className="warning-box">{imageWarnings.join(" ")}</div> : null}

          {imageRows.length ? (
            <div className="preview-table-wrap">
              <table className="draft-table">
                <thead>
                  <tr>
                    {imageDraftFields.map((field) => (
                      <th key={field.key}>{t(field.labelKey)}</th>
                    ))}
                    <th>{t("portfolio.confidence")}</th>
                    <th>{t("portfolio.warnings")}</th>
                  </tr>
                </thead>
                <tbody>
                  {imageRows.map((row, index) => (
                    <tr key={`${row.symbol}-${index}`}>
                      {imageDraftFields.map((field) => (
                        <td key={field.key}>
                          <input
                            className="draft-input"
                            value={(row[field.key] as string | null | undefined) ?? ""}
                            onChange={(event) => updateImageRow(index, field.key, event.target.value)}
                          />
                        </td>
                      ))}
                      <td>
                        <span className={row.confidence === "low" ? "pill warning" : "pill"}>{row.confidence}</span>
                      </td>
                      <td>{row.warnings.length ? row.warnings.join(" ") : "-"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <EmptyState title={t("portfolio.noImagePreviewTitle")}>{t("portfolio.noImagePreviewBody")}</EmptyState>
          )}
        </div>

        {preview ? (
          <div className="import-grid">
            <div className="mapping-grid">
              {requiredMappingFields.map((field) => (
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
            <div className="preview-table-wrap">
              <table>
                <thead>
                  <tr>
                    {preview.headers.map((header) => (
                      <th key={header}>{header}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {preview.sample_rows.map((row, index) => (
                    <tr key={index}>
                      {preview.headers.map((header) => (
                        <td key={header}>{row[header]}</td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {preview.validation_errors.length ? (
              <div className="warning-box">{preview.validation_errors.join(" ")}</div>
            ) : null}
            <button className="primary-button" type="button" onClick={commit}>
              <Check size={18} />
              {t("portfolio.commitImport")}
            </button>
          </div>
        ) : (
          <EmptyState title={t("portfolio.noImportTitle")}>{t("portfolio.noImportBody")}</EmptyState>
        )}
      </section>

      <section className="dashboard-grid">
        <div className="panel">
          <div className="panel-head">
            <h3>{t("portfolio.sectorExposure")}</h3>
          </div>
          {sectorData.length ? (
            <ResponsiveContainer width="100%" height={260}>
              <BarChart data={sectorData}>
                <CartesianGrid strokeDasharray="3 3" vertical={false} />
                <XAxis dataKey="label" />
                <YAxis tickFormatter={(value) => `${value}%`} />
                <Tooltip formatter={(value: number) => percent(value as number)} />
                <Bar dataKey="weight" fill="#2f6f73" radius={[4, 4, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          ) : (
            <EmptyState title={t("portfolio.noExposureTitle")}>{t("portfolio.noExposureBody")}</EmptyState>
          )}
        </div>

        <div className="panel">
          <div className="panel-head">
            <h3>{t("portfolio.refreshResult")}</h3>
          </div>
          {refreshPrices.data ? (
            <div className="result-box">
              <strong>{t("portfolio.refreshedCount", { count: refreshPrices.data.refreshed })}</strong>
              <p>{t("portfolio.failedCount", { count: refreshPrices.data.failed })}</p>
              {refreshPrices.data.failures.map((failure) => (
                <span key={failure}>{failure}</span>
              ))}
            </div>
          ) : (
            <EmptyState title={t("portfolio.noRefreshTitle")}>{t("portfolio.noRefreshBody")}</EmptyState>
          )}
        </div>
      </section>

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
                  <th>{t("portfolio.tableQty")}</th>
                  <th>{t("portfolio.tableAvgCost")}</th>
                  <th>{t("portfolio.tableLastPrice")}</th>
                  <th>{t("portfolio.tableMarketValue")}</th>
                  <th>{t("portfolio.tablePl")}</th>
                  <th>{t("portfolio.tableWeight")}</th>
                  <th>{t("portfolio.tableStatus")}</th>
                </tr>
              </thead>
              <tbody>
                {(positions.data ?? []).map((position) => (
                  <tr key={position.symbol}>
                    <td>
                      <strong>{position.symbol}</strong>
                    </td>
                    <td>{position.name}</td>
                    <td>{number(position.quantity)}</td>
                    <td>{currency(position.average_cost)}</td>
                    <td>{position.last_price ? currency(position.last_price) : "-"}</td>
                    <td>{currency(position.market_value)}</td>
                    <td className={position.unrealized_pnl >= 0 ? "positive-text" : "warning-text"}>
                      {currency(position.unrealized_pnl)}
                    </td>
                    <td>{percent(position.weight)}</td>
                    <td>
                      <span className={position.price_stale ? "pill warning" : "pill"}>
                        {position.price_stale ? t("common.stale") : t("common.fresh")}
                      </span>
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

const requiredMappingFields: Array<{ key: keyof PortfolioImportMapping; labelKey: TranslationKey }> = [
  { key: "symbol", labelKey: "portfolio.mapSymbol" },
  { key: "name", labelKey: "portfolio.mapName" },
  { key: "quantity", labelKey: "portfolio.mapQuantity" },
  { key: "average_cost", labelKey: "portfolio.mapAverageCost" },
  { key: "currency", labelKey: "portfolio.mapCurrency" },
  { key: "account", labelKey: "portfolio.mapAccount" },
  { key: "market", labelKey: "portfolio.mapMarket" },
  { key: "sector", labelKey: "portfolio.mapSector" },
  { key: "imported_market_value", labelKey: "portfolio.mapImportedMarketValue" },
  { key: "notes", labelKey: "portfolio.mapNotes" }
];

type ImageDraftEditableField =
  | "symbol"
  | "name"
  | "quantity"
  | "average_cost"
  | "currency"
  | "account"
  | "market"
  | "sector"
  | "imported_market_value"
  | "notes";

const imageDraftFields: Array<{ key: ImageDraftEditableField; labelKey: TranslationKey }> = [
  { key: "symbol", labelKey: "portfolio.mapSymbol" },
  { key: "name", labelKey: "portfolio.mapName" },
  { key: "quantity", labelKey: "portfolio.mapQuantity" },
  { key: "average_cost", labelKey: "portfolio.mapAverageCost" },
  { key: "currency", labelKey: "portfolio.mapCurrency" },
  { key: "account", labelKey: "portfolio.mapAccount" },
  { key: "market", labelKey: "portfolio.mapMarket" },
  { key: "sector", labelKey: "portfolio.mapSector" },
  { key: "imported_market_value", labelKey: "portfolio.mapImportedMarketValue" },
  { key: "notes", labelKey: "portfolio.mapNotes" }
];

const optionalImageDraftFields: ImageDraftEditableField[] = [
  "account",
  "market",
  "sector",
  "imported_market_value",
  "notes"
];

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

function currency(value: number) {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 2
  }).format(value);
}

function number(value: number) {
  return new Intl.NumberFormat("en-US", { maximumFractionDigits: 4 }).format(value);
}

function percent(value: number) {
  return `${(value * 100).toFixed(1)}%`;
}
