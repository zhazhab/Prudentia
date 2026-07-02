import type {
  MarketValueGroup,
  PortfolioDraftRow,
  PortfolioPosition,
  PortfolioSummary
} from "../types/domain";

export interface MarketGroupDisplay {
  label: string;
  nativeValue: string;
  baseValue: string;
  weightLabel: string;
  stale: boolean;
}

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

export function canCommitDraftRows(rows: PortfolioDraftRow[]) {
  return rows.length > 0 && rows.every((row) => row.errors.length === 0);
}

export function marketGroupsForDisplay(summary: PortfolioSummary): MarketGroupDisplay[] {
  return [...summary.market_groups]
    .sort((left, right) => right.market_value_base - left.market_value_base)
    .map((group) => ({
      label: `${group.market} / ${group.currency}`,
      nativeValue: formatMoney(group.market_value, group.currency),
      baseValue: formatMoney(group.market_value_base, summary.base_currency),
      weightLabel: percent(group.weight),
      stale: summary.fx_rates.some(
        (rate) =>
          rate.stale &&
          rate.from_currency.toUpperCase() === group.currency.toUpperCase() &&
          rate.to_currency.toUpperCase() === summary.base_currency.toUpperCase()
      )
    }));
}

export function formatBaseMoney(summary: PortfolioSummary) {
  return formatMoney(summary.total_market_value_base, summary.base_currency);
}

export function formatNativeGroup(group: MarketValueGroup) {
  return formatMoney(group.market_value, group.currency);
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

export function draftRowHasWarnings(row: PortfolioDraftRow) {
  return row.warnings.length > 0 || row.confidence === "low" || row.confidence === "unknown";
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
    errors: validateDraftRow(next)
  };
}

export function validateDraftRow(row: PortfolioDraftRow) {
  const errors: string[] = [];
  if (!row.symbol.trim()) {
    errors.push("symbol is required");
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
  return errors;
}

export function formatMoney(value: number, currency: string) {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: currency || "USD",
    maximumFractionDigits: 2
  }).format(value);
}

export function percent(value: number) {
  return `${(value * 100).toFixed(1)}%`;
}

function emptyToNull(value: string) {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

const optionalDraftFields: PortfolioDraftEditableField[] = [
  "account",
  "sector",
  "imported_market_value",
  "notes"
];

function isPositiveNumber(value: string) {
  const parsed = Number(value.replaceAll(",", ""));
  return Number.isFinite(parsed) && parsed > 0;
}

function isNonNegativeNumber(value: string) {
  const parsed = Number(value.replaceAll(",", ""));
  return Number.isFinite(parsed) && parsed >= 0;
}
