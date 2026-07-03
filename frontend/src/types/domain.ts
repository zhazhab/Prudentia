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

export interface InvestmentSystem {
  principles: string[];
  checklist_items: string[];
  circle_of_competence: string[];
  decision_rules: string[];
  updated_at: string;
}

export interface InvestmentSystemRefinement extends InvestmentSystem {
  summary: string;
}

export interface Decision {
  id: string;
  memo_id?: string | null;
  symbol?: string | null;
  action: string;
  rationale: string;
  confidence: number;
  expected_outcome: string;
  review_date?: string | null;
  created_at: string;
}

export interface CreateDecisionRequest {
  memo_id?: string | null;
  symbol?: string | null;
  action: string;
  rationale: string;
  confidence: number;
  expected_outcome: string;
  review_date?: string | null;
  decision_date?: string | null;
  quantity?: number | null;
  notional?: number | null;
  price?: number | null;
  currency?: string | null;
  baseline_type?: string | null;
  hypothetical_notional?: number | null;
}

export interface DecisionDeltaLeg {
  id: string;
  decision_id: string;
  leg_kind: string;
  baseline_type?: string | null;
  symbol?: string | null;
  quantity?: number | null;
  notional?: number | null;
  price?: number | null;
  currency: string;
  created_at: string;
  updated_at: string;
}

export interface DecisionDeltaSnapshot {
  id: string;
  decision_id: string;
  as_of_date: string;
  actual_value: number;
  baseline_value: number;
  delta_value: number;
  delta_pct?: number | null;
  portfolio_impact_pct?: number | null;
  price_used?: number | null;
  price_source?: string | null;
  price_updated_at?: string | null;
  fx_rate_used?: number | null;
  fx_source?: string | null;
  fx_updated_at?: string | null;
  price_stale: boolean;
  fx_stale: boolean;
  created_at: string;
}

export interface DecisionDeltaReview {
  decision_id: string;
  notes: string;
  thesis_evidence: string[];
  disconfirming_evidence: string[];
  lessons: string[];
  candidate_principles: string[];
  candidate_checklist_items: string[];
  created_at: string;
  updated_at: string;
}

export interface DecisionDeltaDetail {
  decision: Decision;
  legs: DecisionDeltaLeg[];
  quantifiable: boolean;
  latest_snapshot?: DecisionDeltaSnapshot | null;
  snapshots: DecisionDeltaSnapshot[];
  review?: DecisionDeltaReview | null;
}

export interface DecisionDeltaTimelineSummary {
  label: string;
  visible_decisions_count: number;
  quantifiable_decisions_count: number;
  positive_delta_count: number;
  negative_delta_count: number;
  sum_delta_value: number;
  sum_portfolio_impact_pct?: number | null;
  last_refreshed_at?: string | null;
}

export interface DecisionDeltaTimelineItem {
  decision: Decision;
  quantifiable: boolean;
  reviewed: boolean;
  latest_snapshot?: DecisionDeltaSnapshot | null;
}

export interface DecisionDeltaTimeline {
  summary: DecisionDeltaTimelineSummary;
  items: DecisionDeltaTimelineItem[];
}

export interface DecisionDeltaTimelineFilters {
  symbol?: string;
  action?: string;
  year?: string;
  delta?: string;
  stale?: string;
  reviewed?: string;
  sort?: string;
}

export interface RefreshDecisionDeltasResult {
  refreshed: number;
  failed: number;
  failures: string[];
}

export interface DecisionDeltaReviewRequest {
  notes: string;
  thesis_evidence: string[];
  disconfirming_evidence: string[];
  lessons: string[];
  candidate_principles: string[];
  candidate_checklist_items: string[];
}

export interface AdoptDecisionDeltaCandidatesRequest {
  principles: string[];
  checklist_items: string[];
}

export type ResearchRecordKind = "distillation" | "stock_snapshot" | "portfolio_review";

export interface ResearchRecord {
  id: string;
  kind: ResearchRecordKind;
  title: string;
  source_type?: string | null;
  source_title?: string | null;
  source_author?: string | null;
  source_content?: string | null;
  symbol?: string | null;
  memo_id?: string | null;
  summary: string;
  insights: string[];
  risks: string[];
  checklist: string[];
  candidate_principles: string[];
  candidate_checklist_items: string[];
  raw_output: unknown;
  created_at: string;
  updated_at: string;
}

export interface DistillResearchRequest {
  title: string;
  source_type?: string | null;
  source_title?: string | null;
  source_author?: string | null;
  source_content: string;
  symbol?: string | null;
}

export interface StockSnapshotRequest {
  symbol: string;
  memo_id?: string | null;
}

export interface AdoptResearchCandidatesRequest {
  principles: string[];
  checklist_items: string[];
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

export interface PortfolioImageDraftRow {
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
  confidence: "high" | "medium" | "low" | "unknown" | string;
  warnings: string[];
}

export interface PortfolioImageImportPreview {
  draft_rows: PortfolioDraftRow[];
  rows: PortfolioImageDraftRow[];
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

export interface PortfolioDraftPreview {
  draft_rows: PortfolioDraftRow[];
  warnings: string[];
  source: string;
}

export interface PortfolioImportDraftRequest {
  file_name: string;
  content: string;
  content_encoding?: "base64";
  mapping: PortfolioImportMapping;
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

export interface InvestorProfile {
  level: number;
  xp: number;
  next_level_xp: number;
  attributes: ProfileAttribute[];
  badges: Badge[];
  bias_signals: string[];
  rule_events: string[];
}

export interface ProfileAttribute {
  name: string;
  score: number;
  description: string;
}

export interface Badge {
  name: string;
  description: string;
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
