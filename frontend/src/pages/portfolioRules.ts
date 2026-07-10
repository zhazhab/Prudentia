import type {
  PortfolioDraftRow,
  PortfolioImageImportTask,
  PortfolioPerformanceResponse,
  PortfolioPosition,
  PortfolioSummary
} from "../types/domain";

export interface PositionEditDraft {
  name: string;
  quantity: string;
  average_cost: string;
  currency: string;
  account: string;
  market: string;
  sector: string;
  imported_market_value: string;
  notes: string;
}

export type PortfolioSelectOption = {
  value: string;
  label: string;
};

export const supportedCurrencies = ["CNY", "HKD", "USD"] as const;

export const supportedMarkets: PortfolioSelectOption[] = [
  { value: "A股", label: "A股" },
  { value: "港股", label: "港股" },
  { value: "美股", label: "美股" },
  { value: "Other", label: "Other" }
];

export type PortfolioDraftEditableField =
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

export type PortfolioImportFileKind = "tabular" | "image" | "unsupported";
export type BenchmarkComparisonMetric = "cumulative" | "annualized" | "excess";

export type DraftRowClientIdentity = {
  client_row_id?: string | null;
};

export const draftEditableDisplayFields = [
  "symbol",
  "name",
  "quantity",
  "average_cost",
  "currency",
  "market"
] as const satisfies readonly PortfolioDraftEditableField[];

export const positionTableDisplayFields = [
  "symbol",
  "name",
  "market",
  "quantity",
  "average_cost",
  "market_value",
  "unrealized_pnl",
  "unrealized_pnl_pct",
  "period_return_pct",
  "weight"
] as const;

export type PositionTableDisplayField = (typeof positionTableDisplayFields)[number];

export const positionTableSortableFields = [
  "market_value",
  "unrealized_pnl",
  "unrealized_pnl_pct",
  "period_return_pct",
  "weight"
] as const satisfies readonly PositionTableDisplayField[];

export type PositionSortField = (typeof positionTableSortableFields)[number];
export type PositionSortDirection = "asc" | "desc";

export interface PositionSortRule {
  field: PositionSortField;
  direction: PositionSortDirection;
}

export const defaultPositionSortRule: PositionSortRule = {
  field: "market_value",
  direction: "desc"
};

export const portfolioDashboardPanelIds = ["positions"] as const;

export function sortPositions(positions: PortfolioPosition[], sortRule: PositionSortRule): PortfolioPosition[] {
  const normalizedRule = normalizePositionSortRule(sortRule);
  return [...positions].sort((left, right) => {
    const leftValue = positionSortValue(left, normalizedRule.field);
    const rightValue = positionSortValue(right, normalizedRule.field);
    const leftMissing = leftValue === null || leftValue === undefined;
    const rightMissing = rightValue === null || rightValue === undefined;

    if (leftMissing || rightMissing) {
      if (leftMissing && rightMissing) {
        return left.symbol.localeCompare(right.symbol);
      }
      return leftMissing ? 1 : -1;
    }

    if (leftValue === rightValue) {
      return left.symbol.localeCompare(right.symbol);
    }

    const comparison = leftValue - rightValue;
    return normalizedRule.direction === "asc" ? comparison : -comparison;
  });
}

export function nextPositionSortRule(
  current: PositionSortRule,
  field: PositionSortField
): PositionSortRule {
  const normalizedRule = normalizePositionSortRule(current);
  if (normalizedRule.field !== field) {
    return { field, direction: "desc" };
  }
  return {
    field,
    direction: normalizedRule.direction === "desc" ? "asc" : "desc"
  };
}

export function normalizePositionSortRule(value: unknown): PositionSortRule {
  if (!value || typeof value !== "object") {
    return defaultPositionSortRule;
  }
  const candidate = value as Partial<PositionSortRule>;
  const field = positionTableSortableFields.find((item) => item === candidate.field);
  const direction = candidate.direction === "asc" || candidate.direction === "desc" ? candidate.direction : null;
  return field && direction ? { field, direction } : defaultPositionSortRule;
}

function positionSortValue(position: PortfolioPosition, field: PositionSortField) {
  switch (field) {
    case "market_value":
      return position.market_value_base;
    case "unrealized_pnl":
      return position.unrealized_pnl;
    case "unrealized_pnl_pct":
      return position.unrealized_pnl_pct;
    case "period_return_pct":
      return position.period_return_pct;
    case "weight":
      return position.weight;
  }
}

export function portfolioImportFileKind(file: Pick<File, "name" | "type">): PortfolioImportFileKind {
  const mimeType = file.type.trim().toLowerCase();
  const fileName = file.name.trim().toLowerCase();

  if (supportedImageMimeTypes.has(mimeType) || supportedImageExtensions.some((extension) => fileName.endsWith(extension))) {
    return "image";
  }
  if (supportedTabularMimeTypes.has(mimeType) || supportedTabularExtensions.some((extension) => fileName.endsWith(extension))) {
    return "tabular";
  }
  return "unsupported";
}

export function ensureDraftRowClientIds<T extends DraftRowClientIdentity>(
  rows: T[],
  makeId: () => string
): Array<T & { client_row_id: string }> {
  return rows.map((row) =>
    row.client_row_id ? (row as T & { client_row_id: string }) : { ...row, client_row_id: makeId() }
  );
}

export function canCommitDraftRows(rows: PortfolioDraftRow[]) {
  const rowsForCommit = draftRowsForCommit(rows);
  return rowsForCommit.length > 0 && rowsForCommit.every((row) => row.errors.length === 0);
}

export function emptyPortfolioDraftRow(): PortfolioDraftRow {
  return {
    symbol: "",
    name: "",
    quantity: "",
    average_cost: "",
    currency: "",
    account: null,
    market: "",
    sector: null,
    imported_market_value: null,
    last_price: null,
    notes: null,
    confidence: "unknown",
    warnings: [],
    errors: []
  };
}

export function draftRowsForCommit(rows: PortfolioDraftRow[]) {
  return mergeDuplicateDraftRowsBySymbol(rows.filter((row) => !isBlankDraftRow(row)).map(normalizeDraftRowForCommit));
}

export function mergeDuplicateDraftRowsBySymbol<T extends PortfolioDraftRow>(rows: T[]): T[] {
  const mergedRows: T[] = [];
  const symbolIndexes = new Map<string, number>();

  rows.forEach((row) => {
    const symbol = normalizedSymbol(row);
    if (!symbol || isBlankDraftRow(row)) {
      mergedRows.push(isBlankDraftRow(row) ? ({ ...row, errors: [] } as T) : normalizeMergedDraftRow(row));
      return;
    }

    const existingIndex = symbolIndexes.get(symbol);
    if (existingIndex === undefined) {
      symbolIndexes.set(symbol, mergedRows.length);
      mergedRows.push(normalizeMergedDraftRow(row));
      return;
    }

    mergedRows[existingIndex] = mergeDraftRows(mergedRows[existingIndex], row);
  });

  return mergedRows.map((row) => (isBlankDraftRow(row) ? ({ ...row, errors: [] } as T) : normalizeMergedDraftRow(row)));
}

function mergeDraftRows<T extends PortfolioDraftRow>(left: T, right: T): T {
  const leftQuantity = parseDraftNumber(left.quantity);
  const rightQuantity = parseDraftNumber(right.quantity);
  const totalQuantity =
    leftQuantity !== null && rightQuantity !== null && leftQuantity > 0 && rightQuantity > 0
      ? leftQuantity + rightQuantity
      : null;
  const leftAverageCost = parseDraftNumber(left.average_cost);
  const rightAverageCost = parseDraftNumber(right.average_cost);
  const weightedAverageCost =
    totalQuantity !== null && leftAverageCost !== null && rightAverageCost !== null
      ? (leftAverageCost * leftQuantity! + rightAverageCost * rightQuantity!) / totalQuantity
      : null;
  const leftMarketValue = parseOptionalDraftNumber(left.imported_market_value);
  const rightMarketValue = parseOptionalDraftNumber(right.imported_market_value);
  const leftLastPrice = parseOptionalDraftNumber(left.last_price);
  const rightLastPrice = parseOptionalDraftNumber(right.last_price);
  const leftNativeMarketValue =
    leftLastPrice !== null && leftQuantity !== null ? leftLastPrice * leftQuantity : leftMarketValue;
  const rightNativeMarketValue =
    rightLastPrice !== null && rightQuantity !== null ? rightLastPrice * rightQuantity : rightMarketValue;
  const importedMarketValue =
    leftNativeMarketValue !== null || rightNativeMarketValue !== null
      ? (leftNativeMarketValue ?? 0) + (rightNativeMarketValue ?? 0)
      : null;
  const lastPrice =
    importedMarketValue !== null && totalQuantity !== null
      ? importedMarketValue / totalQuantity
      : leftLastPrice ?? rightLastPrice;

  const errors = [
    ...cleanDraftRowErrors(left.errors),
    ...cleanDraftRowErrors(right.errors),
    ...mergeConflictErrors(left, right)
  ];
  const market = mergeMarketValue(left.market, right.market);

  return {
    ...left,
    symbol: normalizedSymbol(left) || normalizedSymbol(right),
    name: firstPresent(left.name, right.name),
    quantity: totalQuantity === null ? firstPresent(left.quantity, right.quantity) : formatDraftNumber(totalQuantity),
    average_cost:
      weightedAverageCost === null
        ? firstPresent(left.average_cost, right.average_cost)
        : formatDraftNumber(weightedAverageCost),
    currency: firstPresent(left.currency, right.currency).trim().toUpperCase(),
    account: mergeOptionalText(left.account, right.account),
    market,
    sector: mergeOptionalText(left.sector, right.sector),
    imported_market_value:
      importedMarketValue === null ? firstPresentNullable(left.imported_market_value, right.imported_market_value) : formatDraftNumber(importedMarketValue),
    last_price: lastPrice === null ? firstPresentNullable(left.last_price, right.last_price) : formatDraftNumber(lastPrice),
    notes: mergeOptionalText(left.notes, right.notes),
    confidence: mergedConfidence(left.confidence, right.confidence),
    warnings: uniqueText([...left.warnings, ...right.warnings]),
    errors: uniqueText(errors)
  };
}

export function imageImportQueueRunningCount(tasks: PortfolioImageImportTask[]) {
  return tasks.filter((task) => task.status === "running").length;
}

export function imageImportQueueCanStart(tasks: PortfolioImageImportTask[], limit = 2) {
  return imageImportQueueRunningCount(tasks) < limit;
}

export function performanceChartRows(
  data: PortfolioPerformanceResponse | undefined,
  metric: BenchmarkComparisonMetric
): Array<Record<string, string | number | null>> {
  const rows = new Map<string, Record<string, string | number | null>>();
  const portfolioReturnByDate = new Map<string, number | null>();

  data?.series.forEach((point) => {
    const portfolioValue = performancePointValue(point, metric);
    if (point.return_pct !== null && point.return_pct !== undefined) {
      portfolioReturnByDate.set(point.captured_at, point.return_pct);
    }
    rows.set(point.captured_at, {
      label: shortChartDate(point.captured_at),
      ...(metric === "excess" ? {} : { portfolio: percentValue(portfolioValue) })
    });
  });

  data?.benchmarks.forEach((benchmark) => {
    benchmark.series.forEach((point) => {
      const row = rows.get(point.captured_at) ?? {
        label: shortChartDate(point.captured_at)
      };
      row[benchmark.key] =
        metric === "excess"
          ? percentValue(excessReturn(portfolioReturnByDate.get(point.captured_at), point.return_pct))
          : percentValue(performancePointValue(point, metric));
      rows.set(point.captured_at, row);
    });
  });

  return [...rows.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([, row]) => row);
}

export function performanceChartYAxisDomain(
  rows: Array<Record<string, string | number | null>>
): [number, number] {
  const values = rows.flatMap((row) =>
    Object.entries(row)
      .filter(([key, value]) => key !== "label" && typeof value === "number")
      .map(([, value]) => value as number)
  );

  if (!values.length) {
    return [-1, 1];
  }

  const min = Math.min(...values);
  const max = Math.max(...values);
  if (min === max) {
    const padding = Math.max(1, Math.abs(min) * 0.1);
    return [roundDomainValue(min - padding), roundDomainValue(max + padding)];
  }

  const padding = Math.max(1, (max - min) * 0.12);
  return [roundDomainValue(min - padding), roundDomainValue(max + padding)];
}

export function formatBaseMoney(summary: PortfolioSummary) {
  return formatMoney(summary.total_market_value_base, summary.base_currency);
}

export function positionEditDraft(position: PortfolioPosition): PositionEditDraft {
  return {
    name: position.name,
    quantity: String(position.quantity),
    average_cost: String(position.average_cost),
    currency: position.currency,
    account: position.account ?? "",
    market: position.market ?? "",
    sector: position.sector ?? "",
    imported_market_value: String(position.market_value),
    notes: position.notes ?? ""
  };
}

export function positionUpdatePayload(draft: PositionEditDraft) {
  return {
    name: draft.name.trim(),
    quantity: Number(draft.quantity),
    average_cost: Number(draft.average_cost),
    currency: draft.currency.trim().toUpperCase(),
    account: emptyToNull(draft.account),
    market: draft.market.trim(),
    sector: emptyToNull(draft.sector),
    imported_market_value: Number(draft.imported_market_value),
    notes: emptyToNull(draft.notes)
  };
}

export function currencyOptionsForValue(value: string | null | undefined): PortfolioSelectOption[] {
  return selectOptionsForValue(
    supportedCurrencies.map((currency) => ({ value: currency, label: currency })),
    value
  );
}

export function marketOptionsForValue(value: string | null | undefined): PortfolioSelectOption[] {
  return selectOptionsForValue(supportedMarkets, value);
}

export function portfolioIssueLabel(issue: string, locale: "en" | "zh" = "en") {
  const trimmed = issue.trim();
  const normalized = trimmed.toLocaleLowerCase();
  const isZh = locale === "zh";

  if (
    normalized.includes("currency and sector") ||
    (normalized.includes("only visible holding rows") && normalized.includes("symbols/tickers"))
  ) {
    return isZh
      ? "截图里有些字段看不清。我只提取了可见持仓；缺失字段请在表格里补齐。"
      : "Some fields are not visible. Visible holdings were extracted; fill missing fields in the draft table.";
  }
  if (normalized.includes("symbol and currency") && normalized.includes("not visible")) {
    return isZh
      ? "截图里看不到代码和币种；请在表格里补齐后再确认。"
      : "Symbol and currency are not visible; fill them in the draft table before confirming.";
  }
  if (normalized.includes("symbol") && normalized.includes("not visible")) {
    return isZh
      ? "截图里看不到代码；请在表格里补齐后再确认。"
      : "Symbol is not visible; fill it in the draft table before confirming.";
  }
  if (normalized.includes("only") && normalized.includes("visible") && normalized.includes("holding")) {
    return isZh
      ? "只提取了截图中看得见的持仓，隐藏行不会自动补全。"
      : "Only visible holdings were extracted; hidden rows were not inferred.";
  }
  if (normalized === "currency is required") {
    return isZh ? "请补充币种。" : "Currency is required.";
  }
  if (normalized === symbolRequiredError) {
    return isZh ? "请填写代码。" : "Symbol is required.";
  }
  if (normalized === "average_cost must be a number") {
    return isZh ? "平均成本需要是数字。" : "Average cost must be a number.";
  }
  if (normalized === "quantity must be greater than 0") {
    return isZh ? "数量必须大于 0。" : "Quantity must be greater than 0.";
  }
  if (normalized === "imported_market_value must be non-negative") {
    return isZh ? "市值不能为负数。" : "Market value must be non-negative.";
  }
  return trimmed;
}

export function portfolioIssueLabels(issues: string[], locale: "en" | "zh" = "en") {
  return [...new Set(issues.map((issue) => portfolioIssueLabel(issue, locale)).filter(Boolean))];
}

export function updateDraftRowField(
  row: PortfolioDraftRow,
  field: PortfolioDraftEditableField,
  value: string
): PortfolioDraftRow {
  const next = {
    ...row,
    [field]: optionalDraftFields.includes(field) ? value || null : value
  };
  return {
    ...next,
    symbol: next.symbol.trim().toUpperCase(),
    currency: next.currency.trim().toUpperCase(),
    market: next.market.trim(),
    errors: isBlankDraftRow(next) ? [] : validateDraftRow(next)
  };
}

function normalizeDraftRowForCommit(row: PortfolioDraftRow): PortfolioDraftRow {
  const next = {
    symbol: row.symbol.trim().toUpperCase(),
    name: row.name.trim(),
    quantity: row.quantity.trim(),
    average_cost: row.average_cost.trim(),
    currency: row.currency.trim().toUpperCase(),
    account: row.account?.trim() || null,
    market: row.market.trim(),
    sector: row.sector?.trim() || null,
    imported_market_value: row.imported_market_value?.trim() || null,
    last_price: row.last_price?.trim() || null,
    notes: row.notes?.trim() || null,
    confidence: row.confidence,
    warnings: row.warnings,
    errors: []
  };
  const errors = [...validateDraftRow(next), ...cleanDraftRowErrors(row.errors)];
  return {
    ...next,
    errors: [...new Set(errors)]
  };
}

function normalizeMergedDraftRow<T extends PortfolioDraftRow>(row: T): T {
  const errors = isBlankDraftRow(row) ? [] : uniqueText([...validateDraftRow(row), ...cleanDraftRowErrors(row.errors)]);
  return {
    ...row,
    symbol: row.symbol.trim().toUpperCase(),
    currency: row.currency.trim().toUpperCase(),
    market: row.market.trim(),
    errors
  };
}

function isBlankDraftRow(row: PortfolioDraftRow) {
  return draftFieldsForBlankCheck.every((field) => {
    const value = row[field];
    return value === null || value === undefined || String(value).trim() === "";
  });
}

export function validateDraftRow(row: PortfolioDraftRow) {
  const errors: string[] = [];
  if (!row.symbol.trim()) {
    errors.push(symbolRequiredError);
  }
  if (!row.name.trim()) {
    errors.push("name is required");
  }
  if (!isPositiveNumber(row.quantity)) {
    errors.push("quantity must be greater than 0");
  }
  if (!isNonNegativeNumber(row.average_cost)) {
    errors.push("average_cost must be non-negative");
  }
  if (!row.currency.trim()) {
    errors.push("currency is required");
  }
  if (!row.market.trim()) {
    errors.push("market is required");
  }
  if (row.imported_market_value && !isNonNegativeNumber(row.imported_market_value)) {
    errors.push("imported_market_value must be non-negative");
  }
  if (row.last_price && !isNonNegativeNumber(row.last_price)) {
    errors.push("last_price must be non-negative");
  }
  return errors;
}

export function formatMoney(value: number, currency: string) {
  const currencyCode = (currency || "USD").trim().toUpperCase();
  const amount = new Intl.NumberFormat("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2
  }).format(value);
  return `${currencyCode} ${amount}`;
}

export function percent(value: number) {
  return `${(value * 100).toFixed(1)}%`;
}

export function formatReturnPercent(value: number) {
  return `${(value * 100).toFixed(2)}%`;
}

function emptyToNull(value: string) {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

function selectOptionsForValue(
  options: PortfolioSelectOption[],
  value: string | null | undefined
): PortfolioSelectOption[] {
  const currentValue = value?.trim();
  if (!currentValue || options.some((option) => option.value === currentValue)) {
    return options;
  }
  return [...options, { value: currentValue, label: currentValue }];
}

const optionalDraftFields: PortfolioDraftEditableField[] = [
  "account",
  "sector",
  "imported_market_value",
  "notes"
];

const draftFieldsForBlankCheck: PortfolioDraftEditableField[] = [
  "symbol",
  "name",
  "quantity",
  "average_cost",
  "currency",
  "account",
  "market",
  "sector",
  "imported_market_value",
  "notes"
];

const symbolRequiredError = "symbol is required";
const obsoleteDuplicateSymbolErrors = [
  "duplicate symbol must be merged before confirming",
  "duplicate holding identifier must be merged before confirming"
];
const supportedImageMimeTypes = new Set(["image/png", "image/jpeg", "image/jpg", "image/webp"]);
const supportedImageExtensions = [".png", ".jpg", ".jpeg", ".webp"];
const supportedTabularMimeTypes = new Set([
  "text/csv",
  "text/tab-separated-values",
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
]);
const supportedTabularExtensions = [".csv", ".tsv", ".xlsx"];

function normalizedSymbol(row: PortfolioDraftRow) {
  return row.symbol.trim().toUpperCase();
}

function mergeConflictErrors(left: PortfolioDraftRow, right: PortfolioDraftRow) {
  const errors: string[] = [];
  if (left.currency.trim() && right.currency.trim() && left.currency.trim().toUpperCase() !== right.currency.trim().toUpperCase()) {
    errors.push("currency must match across duplicate symbol rows");
  }
  if (left.market.trim() && right.market.trim() && canonicalDraftMarket(left.market) !== canonicalDraftMarket(right.market)) {
    errors.push("market must match across duplicate symbol rows");
  }
  return errors;
}

function cleanDraftRowErrors(errors: string[]) {
  return errors.filter((error) => !obsoleteDuplicateSymbolErrors.includes(error));
}

function firstPresent(left: string, right: string) {
  return left.trim() ? left : right;
}

function firstPresentNullable(left: string | null | undefined, right: string | null | undefined) {
  return left?.trim() ? left : right?.trim() ? right : null;
}

function mergeOptionalText(left: string | null | undefined, right: string | null | undefined) {
  const values = uniqueText([left, right].map((value) => value?.trim() ?? "").filter(Boolean));
  if (values.length === 0) {
    return null;
  }
  return values.join(", ");
}

function mergeMarketValue(left: string, right: string) {
  const leftValue = left.trim();
  const rightValue = right.trim();
  if (!leftValue) {
    return rightValue;
  }
  if (!rightValue) {
    return leftValue;
  }
  const leftCanonical = canonicalDraftMarket(leftValue);
  const rightCanonical = canonicalDraftMarket(rightValue);
  if (leftCanonical === rightCanonical) {
    return leftCanonical;
  }
  return leftValue;
}

function canonicalDraftMarket(value: string) {
  const normalized = value.trim();
  switch (normalized.toUpperCase()) {
    case "US":
    case "USA":
    case "NYSE":
    case "NASDAQ":
    case "美股":
      return "US";
    case "HK":
    case "HKG":
    case "HKEX":
    case "香港":
    case "港股":
      return "HK";
    case "CN":
    case "CHINA":
    case "SH":
    case "SHANGHAI":
    case "SZ":
    case "SHENZHEN":
    case "A股":
    case "沪深":
      return "CN";
    default:
      return normalized;
  }
}

function performancePointValue(
  point: { return_pct?: number | null; annualized_return_pct?: number | null },
  metric: BenchmarkComparisonMetric
) {
  return metric === "annualized" ? point.annualized_return_pct : point.return_pct;
}

function excessReturn(portfolioReturn: number | null | undefined, benchmarkReturn: number | null | undefined) {
  if (portfolioReturn === null || portfolioReturn === undefined || benchmarkReturn === null || benchmarkReturn === undefined) {
    return null;
  }
  return portfolioReturn - benchmarkReturn;
}

function percentValue(value: number | null | undefined) {
  return value === null || value === undefined ? null : Number((value * 100).toFixed(2));
}

function shortChartDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value.slice(5, 10);
  }
  return new Intl.DateTimeFormat("zh-CN", {
    timeZone: "Asia/Shanghai",
    month: "2-digit",
    day: "2-digit"
  }).format(date);
}

function roundDomainValue(value: number) {
  return Number(value.toFixed(2));
}

function uniqueText(values: string[]) {
  return [...new Set(values.map((value) => value.trim()).filter(Boolean))];
}

function mergedConfidence(left: string, right: string) {
  const rank: Record<string, number> = { high: 0, medium: 1, low: 2, unknown: 3 };
  return (rank[left] ?? 3) >= (rank[right] ?? 3) ? left : right;
}

function parseDraftNumber(value: string) {
  const parsed = Number(value.replaceAll(",", ""));
  return Number.isFinite(parsed) ? parsed : null;
}

function parseOptionalDraftNumber(value: string | null | undefined) {
  if (!value?.trim()) {
    return null;
  }
  return parseDraftNumber(value);
}

function formatDraftNumber(value: number) {
  return Number(value.toFixed(6)).toString();
}

function isPositiveNumber(value: string) {
  const parsed = Number(value.replaceAll(",", ""));
  return Number.isFinite(parsed) && parsed > 0;
}

function isNonNegativeNumber(value: string) {
  const parsed = Number(value.replaceAll(",", ""));
  return Number.isFinite(parsed) && parsed >= 0;
}
