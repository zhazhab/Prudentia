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

export type MemoThreadMessageRole = "user" | "assistant" | "system";
export type MemoThreadMessageStatus = "pending" | "streaming" | "completed" | "canceled" | "failed";

export type ConversationCapabilityKind = "skill" | "agent";
export type ConversationCapabilityStage = "analysis" | "challenge";

export interface ConversationCapabilityFinding {
  category: string;
  title: string;
  judgment: string;
  claimType: "fact" | "inference" | "hypothesis" | string;
  evidence: ConversationCapabilityEvidence[];
  causalChain: string[];
  counterargument: string;
  unknowns: string[];
  confidence: "low" | "medium" | "high" | string;
  leadingIndicators: string[];
  falsification: string;
  decisionImpact: string;
}

export interface ConversationCapabilityEvidence {
  claim: string;
  sourceUrls: string[];
  asOf: string;
}

export interface ConversationCapabilityEvidenceAssessment {
  status: "sufficient" | "partial" | "insufficient" | string;
  rationale: string;
  latestEvidenceDate: string;
  criticalGaps: string[];
}

export interface ConversationCapabilityArtifact {
  call_id: string;
  capability_id: string;
  capability_version: number;
  capability_kind: ConversationCapabilityKind;
  display_name: string;
  artifact_type: string;
  status: "completed" | "failed";
  subject_label?: string | null;
  payload: Record<string, unknown>;
  source_ids: string[];
  warning?: string | null;
  error_code?: string | null;
  error_message?: string | null;
  provider?: string | null;
  model?: string | null;
  model_steps: Array<{ step: number; provider: string; model: string }>;
  manifest_hash: string;
  duration_ms: number;
  execution_steps: number;
  agent_trace?: AgentExecutionTrace[];
}

export interface AgentExecutionTrace {
  turn: number;
  action: "tool" | "final" | string;
  tool_id?: string | null;
  tool_version?: number | null;
  tool_display_name?: string | null;
  status: "completed" | "failed" | string;
  source_count: number;
  error_code?: string | null;
}

export interface ActiveCapabilityCall {
  callId: string;
  capabilityId: string;
  capabilityKind: "native" | ConversationCapabilityKind;
  displayName: string;
  stage: "research" | ConversationCapabilityStage;
  activity: string;
  subject?: string;
  stepIndex?: number;
  stepTotal?: number;
  nestedToolName?: string;
  nestedToolDisplayName?: string;
  agentTurn?: number;
  agentTurnLimit?: number;
}

export interface ConversationActiveCapability {
  call_id: string;
  tool_name: string;
  tool_version: number;
  capability_kind: "native" | ConversationCapabilityKind;
  display_name: string;
  stage: "research" | ConversationCapabilityStage;
  activity: string;
  subject_label?: string | null;
  step_index: number;
  total_steps: number;
  nested_tool_name?: string | null;
  nested_tool_display_name?: string | null;
  agent_turn?: number | null;
  agent_turn_limit?: number | null;
}

export interface ConversationExecutionPlanStep {
  id: string;
  status: "pending" | "running" | "completed" | "skipped" | "failed" | string;
}

export interface ConversationExecutionPlan {
  template_id: string;
  scope: string;
  dimensions: string[];
  steps: ConversationExecutionPlanStep[];
}

export interface MemoThreadSummary {
  id: string;
  title: string;
  summary: string;
  status: string;
  linked_symbols: string[];
  tags: string[];
  archived_at?: string | null;
  created_at: string;
  updated_at: string;
  last_message_at: string;
}

export interface MemoThreadMessage {
  id: string;
  thread_id: string;
  role: MemoThreadMessageRole;
  content: string;
  status: MemoThreadMessageStatus;
  request_id?: string | null;
  duration_ms?: number | null;
  artifacts: unknown[];
  sources: unknown[];
  used_context: unknown[];
  created_at: string;
  updated_at: string;
}

export interface MemoThreadDetail {
  thread: MemoThreadSummary;
  messages: MemoThreadMessage[];
  has_more: boolean;
}

export type ConversationSubjectKind = "company" | "investment_system" | "psychology" | "general";

export interface ThreadSubject {
  kind: ConversationSubjectKind;
  subject_key?: string | null;
  label?: string | null;
  confidence: number;
}

export type ConversationRunStatus =
  | "queued"
  | "running"
  | "completed"
  | "failed"
  | "canceled"
  | "interrupted";

export type ConversationRunPhase =
  | "queued"
  | "resolving_subject"
  | "loading_context"
  | "researching"
  | "generating"
  | "extracting_actions"
  | "persisting"
  | "completed"
  | "failed"
  | "canceled"
  | "interrupted";

export type TaskRouteReason =
  | "social_turn"
  | "short_question"
  | "subject_clarification"
  | "attachment_analysis"
  | "investment_system"
  | "multi_part_request"
  | "long_request"
  | "explicit_deep_analysis"
  | "company_research"
  | "standard_conversation";

export interface ConversationRun {
  id: string;
  client_request_id: string;
  thread_id: string;
  user_message_id: string;
  assistant_message_id?: string | null;
  retry_of_run_id?: string | null;
  status: ConversationRunStatus;
  phase: ConversationRunPhase;
  provider?: string | null;
  task_complexity?: "simple" | "standard" | "deep" | string | null;
  model?: string | null;
  route_reason?: TaskRouteReason | null;
  activity?: string | null;
  source_count?: number | null;
  error_code?: string | null;
  error_message?: string | null;
  started_at: string;
  updated_at: string;
  finished_at?: string | null;
  active_capabilities?: ConversationActiveCapability[];
  execution_plan?: ConversationExecutionPlan | null;
}

export interface RunEvent {
  event_id: number;
  run_id: string;
  thread_id: string;
  event_type: string;
  payload: Record<string, unknown>;
  created_at: string;
}

export interface ConversationAction {
  id: string;
  run_id: string;
  assistant_message_id?: string | null;
  thread_id: string;
  action_type: "company_view_patch" | "trade_record" | "rule_graph_patch" | string;
  title: string;
  rationale: string;
  payload: Record<string, unknown>;
  result?: unknown;
  target_version?: number | null;
  status: "proposed" | "edited" | "executing" | "executed" | "rejected" | "failed" | string;
  error?: string | null;
  created_at: string;
  updated_at: string;
  executed_at?: string | null;
}

export interface ConversationThreadSummary extends MemoThreadSummary {
  subject: ThreadSubject;
  active_run?: ConversationRun | null;
}

export interface CompanyViewSections {
  business_quality: string;
  moat: string;
  financials: string;
  valuation_expectations: string;
  thesis: string;
  risks: string;
  catalysts: string;
  disconfirming_evidence: string;
  open_questions: string[];
}

export interface CompanyView {
  symbol: string;
  company_name: string;
  current_version: number;
  content: CompanyViewSections;
  markdown_path: string;
  updated_at: string;
}

export interface ConversationThreadDetail {
  thread: ConversationThreadSummary;
  latest_run?: ConversationRun | null;
  messages: MemoThreadMessage[];
  actions: ConversationAction[];
  company_view?: CompanyView | null;
  has_more: boolean;
}

export interface StartConversationRunResponse {
  run: ConversationRun;
  thread: ConversationThreadSummary;
}

export interface ConversationAttachment {
  id: string;
  content_hash: string;
  file_name: string;
  mime_type: string;
  relative_path?: string | null;
  source_url?: string | null;
  extracted_text?: string | null;
  parse_status: "parsed" | "ready" | "stored" | "failed" | string;
  parse_error?: string | null;
  size_bytes: number;
  created_at: string;
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
  market_value_base: number;
  unrealized_pnl: number;
  unrealized_pnl_pct?: number | null;
  period_profit_loss_base?: number | null;
  period_return_pct?: number | null;
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
  net_cash_flow_base: number;
  return_pct?: number | null;
  simple_return_pct?: number | null;
  annualized_return_pct?: number | null;
  return_method: "time_weighted" | string;
}

export interface PortfolioPerformancePoint {
  captured_at: string;
  value_base: number;
  profit_loss_base?: number | null;
  net_cash_flow_base: number;
  return_pct?: number | null;
  simple_return_pct?: number | null;
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
  source?: string | null;
  series: BenchmarkPerformancePoint[];
}

export interface BenchmarkPerformancePoint {
  captured_at: string;
  value_base?: number | null;
  return_pct?: number | null;
  annualized_return_pct?: number | null;
  stale: boolean;
  error?: string | null;
  source?: string | null;
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
  provider_chain: string[];
  openai_base_url: string;
  openai_model: string;
  openai_model_simple: string;
  openai_model_standard: string;
  openai_model_deep: string;
  has_openai_api_key: boolean;
  cli_provider: "codex" | string;
  cli_path: string;
  cli_model?: string | null;
  cli_model_simple: string;
  cli_model_standard: string;
  cli_model_deep: string;
  cli_profile?: string | null;
  cli_login_command?: string | null;
}

export interface UpdateAiSettings {
  provider?: string;
  openai_api_key?: string;
  openai_base_url?: string;
  openai_model?: string;
  openai_model_simple?: string;
  openai_model_standard?: string;
  openai_model_deep?: string;
  cli_provider?: string;
  cli_path?: string;
  cli_model?: string;
  cli_model_simple?: string;
  cli_model_standard?: string;
  cli_model_deep?: string;
  cli_profile?: string;
  persist_to_env?: boolean;
}
