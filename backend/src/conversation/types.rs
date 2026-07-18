use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::memo_thread::{MemoThreadMessage, MemoThreadSummary};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThreadSubject {
    pub kind: String,
    pub subject_key: Option<String>,
    pub label: Option<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadSubjectKind {
    Company,
    InvestmentSystem,
    Psychology,
    General,
    Unknown,
}

impl ThreadSubject {
    pub fn kind_type(&self) -> ThreadSubjectKind {
        match self.kind.as_str() {
            "company" => ThreadSubjectKind::Company,
            "investment_system" => ThreadSubjectKind::InvestmentSystem,
            "psychology" => ThreadSubjectKind::Psychology,
            "general" => ThreadSubjectKind::General,
            _ => ThreadSubjectKind::Unknown,
        }
    }
}

impl Default for ThreadSubject {
    fn default() -> Self {
        Self {
            kind: "general".to_string(),
            subject_key: None,
            label: None,
            confidence: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRun {
    pub id: String,
    pub client_request_id: String,
    pub thread_id: String,
    pub user_message_id: String,
    pub assistant_message_id: Option<String>,
    pub retry_of_run_id: Option<String>,
    pub status: String,
    pub phase: String,
    pub provider: Option<String>,
    pub task_complexity: Option<String>,
    pub model: Option<String>,
    pub route_reason: Option<String>,
    pub activity: Option<String>,
    pub source_count: Option<i64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub started_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    #[serde(default)]
    pub active_capabilities: Vec<ConversationActiveCapability>,
    #[serde(default)]
    pub execution_plan: Option<ConversationExecutionPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationExecutionPlan {
    pub template_id: String,
    pub scope: String,
    pub dimensions: Vec<String>,
    pub steps: Vec<ConversationExecutionPlanStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationExecutionPlanStep {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationActiveCapability {
    pub call_id: String,
    pub tool_name: String,
    pub tool_version: u16,
    pub capability_kind: String,
    pub display_name: String,
    pub stage: String,
    pub activity: String,
    pub subject_label: Option<String>,
    pub step_index: usize,
    pub total_steps: usize,
    #[serde(default)]
    pub nested_tool_name: Option<String>,
    #[serde(default)]
    pub nested_tool_display_name: Option<String>,
    #[serde(default)]
    pub agent_turn: Option<u8>,
    #[serde(default)]
    pub agent_turn_limit: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    pub event_id: i64,
    pub run_id: String,
    pub thread_id: String,
    pub event_type: String,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationCapabilitySummary {
    pub id: String,
    pub version: u16,
    pub kind: String,
    pub stage: String,
    pub display_name: String,
    pub description: String,
    pub artifact_type: String,
    pub model_tier: Option<String>,
    pub max_steps: u8,
    #[serde(default)]
    pub loaded_skills: Vec<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    pub surfaces: Vec<String>,
    pub subjects: Vec<String>,
    pub manifest_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedResearchSource {
    pub id: String,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source_tier: String,
    pub retrieved_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationAction {
    pub id: String,
    pub run_id: String,
    pub assistant_message_id: Option<String>,
    pub thread_id: String,
    pub action_type: String,
    pub title: String,
    pub rationale: String,
    pub payload: Value,
    pub result: Option<Value>,
    pub target_version: Option<i64>,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub executed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationThreadSummary {
    #[serde(flatten)]
    pub thread: MemoThreadSummary,
    pub subject: ThreadSubject,
    pub active_run: Option<ConversationRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationThreadDetail {
    pub thread: ConversationThreadSummary,
    pub latest_run: Option<ConversationRun>,
    pub messages: Vec<MemoThreadMessage>,
    pub actions: Vec<ConversationAction>,
    pub company_view: Option<CompanyView>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompanyViewSections {
    #[serde(default)]
    pub business_quality: String,
    #[serde(default)]
    pub moat: String,
    #[serde(default)]
    pub financials: String,
    #[serde(default)]
    pub valuation_expectations: String,
    #[serde(default)]
    pub thesis: String,
    #[serde(default)]
    pub risks: String,
    #[serde(default)]
    pub catalysts: String,
    #[serde(default)]
    pub disconfirming_evidence: String,
    #[serde(default)]
    pub open_questions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyView {
    pub symbol: String,
    pub company_name: String,
    pub current_version: i64,
    pub content: CompanyViewSections,
    pub markdown_path: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyViewVersion {
    pub symbol: String,
    pub version: i64,
    pub content: CompanyViewSections,
    pub action_id: Option<String>,
    pub provenance: Value,
    pub markdown_path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RollbackCompanyViewRequest {
    pub expected_version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyViewPatch {
    pub symbol: String,
    pub company_name: String,
    #[serde(default)]
    pub base_version: i64,
    pub changes: CompanyViewChanges,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompanyViewChanges {
    pub business_quality: Option<String>,
    pub moat: Option<String>,
    pub financials: Option<String>,
    pub valuation_expectations: Option<String>,
    pub thesis: Option<String>,
    pub risks: Option<String>,
    pub catalysts: Option<String>,
    pub disconfirming_evidence: Option<String>,
    pub open_questions: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartRunRequest {
    pub client_request_id: String,
    pub thread_id: Option<String>,
    pub client_thread_id: Option<String>,
    pub content: String,
    #[serde(default)]
    pub attachment_ids: Vec<String>,
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartRunResponse {
    pub run: ConversationRun,
    pub thread: ConversationThreadSummary,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSubjectRequest {
    pub kind: String,
    pub subject_key: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateActionRequest {
    pub payload: Value,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConfirmActionRequest {
    pub expected_version: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UploadAttachmentRequest {
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
    pub content: Option<String>,
    pub content_encoding: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationAttachment {
    pub id: String,
    pub content_hash: String,
    pub file_name: String,
    pub mime_type: String,
    pub relative_path: Option<String>,
    pub source_url: Option<String>,
    pub extracted_text: Option<String>,
    pub parse_status: String,
    pub parse_error: Option<String>,
    pub size_bytes: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct EventQuery {
    pub after_event_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ThreadDetailQuery {
    pub message_limit: Option<i64>,
    pub before_message_id: Option<String>,
}
