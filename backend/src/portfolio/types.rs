#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioPosition {
    pub symbol: String,
    pub name: String,
    pub asset_type: String,
    pub quantity: f64,
    pub average_cost: f64,
    pub currency: String,
    pub account: Option<String>,
    pub market: Option<String>,
    pub sector: Option<String>,
    pub notes: Option<String>,
    pub last_price: Option<f64>,
    pub market_value: f64,
    pub unrealized_pnl: f64,
    pub weight: f64,
    pub price_updated_at: Option<String>,
    pub price_stale: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PortfolioImportMapping {
    pub symbol: String,
    pub name: String,
    pub quantity: String,
    pub average_cost: String,
    pub currency: String,
    pub account: Option<String>,
    pub market: Option<String>,
    pub sector: Option<String>,
    pub imported_market_value: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioImportPreviewRequest {
    pub file_name: String,
    pub content: String,
    pub content_encoding: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioImportPreview {
    pub headers: Vec<String>,
    pub sample_rows: Vec<HashMap<String, String>>,
    pub suggested_mapping: PortfolioImportMapping,
    pub validation_errors: Vec<String>,
    pub draft_rows: Vec<PortfolioDraftRow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioImportDraftRequest {
    pub file_name: String,
    pub content: String,
    pub content_encoding: Option<String>,
    pub mapping: PortfolioImportMapping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioDraftRow {
    pub symbol: String,
    pub name: String,
    pub quantity: String,
    pub average_cost: String,
    pub currency: String,
    pub account: Option<String>,
    pub market: String,
    pub sector: Option<String>,
    pub imported_market_value: Option<String>,
    #[serde(default)]
    pub last_price: Option<String>,
    pub notes: Option<String>,
    pub confidence: String,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioDraftPreview {
    pub draft_rows: Vec<PortfolioDraftRow>,
    pub warnings: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioDraftCommitRequest {
    pub rows: Vec<PortfolioDraftRow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioImageImportPreviewRequest {
    pub file_name: String,
    pub content: String,
    pub content_encoding: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioImageDraftRow {
    pub symbol: String,
    pub name: String,
    pub quantity: String,
    pub average_cost: String,
    pub currency: String,
    pub account: Option<String>,
    pub market: Option<String>,
    pub sector: Option<String>,
    pub imported_market_value: Option<String>,
    #[serde(default)]
    pub last_price: Option<String>,
    pub notes: Option<String>,
    pub confidence: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioImageRecognition {
    pub rows: Vec<PortfolioImageDraftRow>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioImageImportPreview {
    pub draft_rows: Vec<PortfolioDraftRow>,
    pub rows: Vec<PortfolioImageDraftRow>,
    pub warnings: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioImportCommitRequest {
    pub file_name: String,
    pub content: String,
    pub content_encoding: Option<String>,
    pub mapping: PortfolioImportMapping,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioImportResult {
    pub imported_count: usize,
    pub skipped_count: usize,
    pub positions: Vec<PortfolioPosition>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioSummary {
    pub total_market_value: f64,
    pub total_cost: f64,
    pub total_unrealized_pnl: f64,
    pub positions_count: usize,
    pub price_stale_count: usize,
    pub top_positions: Vec<WeightSlice>,
    pub sectors: Vec<WeightSlice>,
    pub market_groups: Vec<MarketValueGroup>,
    pub base_currency: String,
    pub total_market_value_base: f64,
    pub total_cost_base: f64,
    pub total_unrealized_pnl_base: f64,
    pub fx_rates: Vec<PortfolioFxRate>,
    pub fx_stale_count: usize,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeightSlice {
    pub label: String,
    pub value: f64,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketValueGroup {
    pub market: String,
    pub currency: String,
    pub market_value: f64,
    pub cost: f64,
    pub unrealized_pnl: f64,
    pub market_value_base: f64,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioFxRate {
    pub from_currency: String,
    pub to_currency: String,
    pub rate: f64,
    pub source: String,
    pub updated_at: String,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PriceRefreshResult {
    pub refreshed: usize,
    pub failed: usize,
    pub failures: Vec<String>,
    pub positions: Vec<PortfolioPosition>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioPerformanceQuery {
    pub period: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioPerformanceResponse {
    pub period: String,
    pub base_currency: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub partial_period: bool,
    pub portfolio: PortfolioPerformanceMetric,
    pub series: Vec<PortfolioPerformancePoint>,
    pub benchmarks: Vec<BenchmarkPerformance>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioPerformanceMetric {
    pub start_value_base: Option<f64>,
    pub end_value_base: Option<f64>,
    pub profit_loss_base: Option<f64>,
    pub return_pct: Option<f64>,
    pub annualized_return_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioPerformancePoint {
    pub captured_at: String,
    pub value_base: f64,
    pub profit_loss_base: Option<f64>,
    pub return_pct: Option<f64>,
    pub annualized_return_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkPerformance {
    pub key: String,
    pub label: String,
    pub symbol: String,
    pub available: bool,
    pub stale: bool,
    pub start_value_base: Option<f64>,
    pub end_value_base: Option<f64>,
    pub return_pct: Option<f64>,
    pub annualized_return_pct: Option<f64>,
    pub error: Option<String>,
    pub series: Vec<BenchmarkPerformancePoint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkPerformancePoint {
    pub captured_at: String,
    pub value_base: Option<f64>,
    pub return_pct: Option<f64>,
    pub annualized_return_pct: Option<f64>,
    pub stale: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdatePortfolioPositionRequest {
    pub name: Option<String>,
    pub quantity: Option<f64>,
    pub average_cost: Option<f64>,
    pub currency: Option<String>,
    pub account: Option<String>,
    pub market: Option<String>,
    pub sector: Option<String>,
    pub imported_market_value: Option<f64>,
    pub notes: Option<String>,
}
