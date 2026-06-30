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
  updated_at: string;
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
