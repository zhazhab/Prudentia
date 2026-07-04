export interface Memo {
  id: string;
  title: string;
  symbol?: string | null;
  asset_type: string;
  thesis: string;
  risks: string;
  catalysts: string;
  disconfirming_evidence: string;
  notes: string;
  status: string;
  tags: string[];
  created_at: string;
  updated_at: string;
}

export interface MemoExtraction {
  thesis: string;
  risks: string;
  catalysts: string;
  disconfirming_evidence: string;
  checklist: string[];
}

export interface PortfolioPosition {
  symbol: string;
  name: string;
  asset_type: string;
  quantity: number;
  average_cost: number;
  currency: string;
  account?: string | null;
  market?: string | null;
  sector?: string | null;
  notes?: string | null;
  last_price?: number | null;
  market_value: number;
  unrealized_pnl: number;
  weight: number;
  price_updated_at?: string | null;
  price_stale: boolean;
  updated_at: string;
}

export interface WeightSlice {
  label: string;
  value: number;
  weight: number;
}

export interface PortfolioSummary {
  total_market_value: number;
  total_cost: number;
  total_unrealized_pnl: number;
  positions_count: number;
  price_stale_count: number;
  top_positions: WeightSlice[];
  sectors: WeightSlice[];
  market_groups: MarketValueGroup[];
  base_currency: string;
  total_market_value_base: number;
  total_cost_base: number;
  total_unrealized_pnl_base: number;
  fx_rates: PortfolioFxRate[];
  fx_stale_count: number;
  updated_at: string;
}

export interface MarketValueGroup {
  market: string;
  currency: string;
  market_value: number;
  cost: number;
  unrealized_pnl: number;
  market_value_base: number;
  weight: number;
}

export interface PortfolioFxRate {
  from_currency: string;
  to_currency: string;
  rate: number;
  source: string;
  updated_at: string;
  stale: boolean;
}

export interface PortfolioImportMapping {
  symbol: string;
  name: string;
  quantity: string;
  average_cost: string;
  currency: string;
  account?: string | null;
  market?: string | null;
  sector?: string | null;
  imported_market_value?: string | null;
  notes?: string | null;
}

export interface PortfolioImportPreview {
  headers: string[];
  sample_rows: Record<string, string>[];
  suggested_mapping: PortfolioImportMapping;
  validation_errors: string[];
  draft_rows: PortfolioDraftRow[];
}

export interface PortfolioDraftRow {
  symbol: string;
  name: string;
  quantity: string;
  average_cost: string;
  currency: string;
  account?: string | null;
  market: string;
  sector?: string | null;
  imported_market_value?: string | null;
  last_price?: string | null;
  notes?: string | null;
  confidence: "high" | "medium" | "low" | "unknown" | string;
  warnings: string[];
  errors: string[];
}

export type PortfolioImageImportTaskStatus = "queued" | "running" | "completed" | "failed" | "canceled";

export interface PortfolioImageImportTask {
  id: string;
  file_name: string;
  status: PortfolioImageImportTaskStatus;
  stage: string | null;
  elapsed_ms: number;
  recognized_rows: number;
  error: string | null;
}

export interface PortfolioImageImportPreview {
  draft_rows: PortfolioDraftRow[];
  warnings: string[];
  source: string;
}

export interface PortfolioImportResult {
  imported_count: number;
  skipped_count: number;
  positions: PortfolioPosition[];
}

export interface PriceRefreshResult {
  refreshed: number;
  failed: number;
  failures: string[];
  positions: PortfolioPosition[];
}

export type PortfolioPerformancePeriod = "month" | "year" | "since_inception";

export interface PortfolioPerformanceResponse {
  period: PortfolioPerformancePeriod;
  base_currency: string;
  start_date?: string | null;
  end_date?: string | null;
  partial_period: boolean;
  portfolio: PortfolioPerformanceMetric;
  series: PortfolioPerformancePoint[];
  benchmarks: BenchmarkPerformance[];
  updated_at: string;
}

export interface PortfolioPerformanceMetric {
  start_value_base?: number | null;
  end_value_base?: number | null;
  profit_loss_base?: number | null;
  return_pct?: number | null;
  annualized_return_pct?: number | null;
}

export interface PortfolioPerformancePoint {
  captured_at: string;
  value_base: number;
  profit_loss_base?: number | null;
  return_pct?: number | null;
  annualized_return_pct?: number | null;
}

export interface BenchmarkPerformance {
  key: string;
  label: string;
  symbol: string;
  available: boolean;
  stale: boolean;
  start_value_base?: number | null;
  end_value_base?: number | null;
  return_pct?: number | null;
  annualized_return_pct?: number | null;
  error?: string | null;
  series: BenchmarkPerformancePoint[];
}

export interface BenchmarkPerformancePoint {
  captured_at: string;
  value_base?: number | null;
  return_pct?: number | null;
  annualized_return_pct?: number | null;
  stale: boolean;
  error?: string | null;
}

export interface PortfolioDraftPreview {
  draft_rows: PortfolioDraftRow[];
  warnings: string[];
  source: string;
}

export interface PortfolioDraftSymbolResolveResult {
  draft_rows: PortfolioDraftRow[];
  resolved_count: number;
}

export interface PortfolioDraftCommitRequest {
  rows: PortfolioDraftRow[];
}

export interface UpdatePortfolioPosition {
  name?: string;
  quantity?: number;
  average_cost?: number;
  currency?: string;
  account?: string | null;
  market?: string;
  sector?: string | null;
  imported_market_value?: number;
  notes?: string | null;
}

export interface AiSettings {
  provider: "mock" | "openai" | "cli" | string;
  openai_base_url: string;
  openai_model: string;
  has_openai_api_key: boolean;
  cli_provider: "codex" | string;
  cli_path: string;
  cli_model?: string | null;
  cli_profile?: string | null;
  cli_login_command?: string | null;
}

export interface UpdateAiSettings {
  provider?: string;
  openai_api_key?: string;
  openai_base_url?: string;
  openai_model?: string;
  cli_provider?: string;
  cli_path?: string;
  cli_model?: string;
  cli_profile?: string;
  persist_to_env?: boolean;
}
