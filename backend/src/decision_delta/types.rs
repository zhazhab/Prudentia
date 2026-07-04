#[derive(Debug, Clone)]
pub struct DecisionDeltaInput {
    pub action: String,
    pub symbol: Option<String>,
    pub quantity: Option<f64>,
    pub notional: Option<f64>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub baseline_type: Option<String>,
    pub hypothetical_notional: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaLeg {
    pub id: String,
    pub decision_id: String,
    pub leg_kind: String,
    pub baseline_type: Option<String>,
    pub symbol: Option<String>,
    pub quantity: Option<f64>,
    pub notional: Option<f64>,
    pub price: Option<f64>,
    pub currency: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaSnapshot {
    pub id: String,
    pub decision_id: String,
    pub as_of_date: String,
    pub actual_value: f64,
    pub baseline_value: f64,
    pub delta_value: f64,
    pub delta_pct: Option<f64>,
    pub portfolio_impact_pct: Option<f64>,
    pub price_used: Option<f64>,
    pub price_source: Option<String>,
    pub price_updated_at: Option<String>,
    pub fx_rate_used: Option<f64>,
    pub fx_source: Option<String>,
    pub fx_updated_at: Option<String>,
    pub price_stale: bool,
    pub fx_stale: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaReview {
    pub decision_id: String,
    pub notes: String,
    pub thesis_evidence: Vec<String>,
    pub disconfirming_evidence: Vec<String>,
    pub lessons: Vec<String>,
    pub candidate_principles: Vec<String>,
    pub candidate_checklist_items: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaDetail {
    pub decision: Decision,
    pub legs: Vec<DecisionDeltaLeg>,
    pub quantifiable: bool,
    pub latest_snapshot: Option<DecisionDeltaSnapshot>,
    pub snapshots: Vec<DecisionDeltaSnapshot>,
    pub review: Option<DecisionDeltaReview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaTimeline {
    pub summary: DecisionDeltaTimelineSummary,
    pub items: Vec<DecisionDeltaTimelineItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaTimelineSummary {
    pub label: String,
    pub visible_decisions_count: usize,
    pub quantifiable_decisions_count: usize,
    pub positive_delta_count: usize,
    pub negative_delta_count: usize,
    pub sum_delta_value: f64,
    pub sum_portfolio_impact_pct: Option<f64>,
    pub last_refreshed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaTimelineItem {
    pub decision: Decision,
    pub quantifiable: bool,
    pub reviewed: bool,
    pub latest_snapshot: Option<DecisionDeltaSnapshot>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DecisionDeltaTimelineQuery {
    pub symbol: Option<String>,
    pub action: Option<String>,
    pub year: Option<String>,
    pub delta: Option<String>,
    pub stale: Option<String>,
    pub reviewed: Option<String>,
    pub sort: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DecisionDeltaDetailQuery {
    snapshot_limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RefreshDecisionDeltasRequest {
    pub decision_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefreshDecisionDeltasResult {
    pub refreshed: usize,
    pub failed: usize,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DecisionDeltaReviewRequest {
    pub notes: String,
    pub thesis_evidence: Vec<String>,
    pub disconfirming_evidence: Vec<String>,
    pub lessons: Vec<String>,
    pub candidate_principles: Vec<String>,
    pub candidate_checklist_items: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdoptDecisionDeltaCandidatesRequest {
    pub principles: Vec<String>,
    pub checklist_items: Vec<String>,
}
