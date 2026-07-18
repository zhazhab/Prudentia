struct TestTool {
    name: &'static str,
    side_effect: ToolSideEffect,
    confirmation: ToolConfirmation,
    invoked: Arc<AtomicBool>,
}

struct ConcurrentTool {
    name: &'static str,
    active: Arc<AtomicUsize>,
    maximum: Arc<AtomicUsize>,
}

impl ConcurrentTool {
    fn new(name: &'static str, active: Arc<AtomicUsize>, maximum: Arc<AtomicUsize>) -> Self {
        Self {
            name,
            active,
            maximum,
        }
    }
}

#[async_trait]
impl ConversationTool for ConcurrentTool {
    fn descriptor(&self) -> ToolDescriptor {
        planning_descriptor(self.name, CapabilityKind::Skill, &[])
    }

    async fn invoke(
        &self,
        _arguments: Value,
        _context: CapabilityExecutionContext,
        _progress: mpsc::UnboundedSender<ToolProgress>,
    ) -> Result<ToolOutput, ToolExecutionError> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum.fetch_max(active, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(40)).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(ToolOutput {
            artifact_type: "parallel_test".to_string(),
            payload: json!({}),
            sources: Vec::new(),
            warning: None,
            model_route: None,
            model_routes: Vec::new(),
            execution_steps: 1,
            agent_trace: Vec::new(),
        })
    }
}

impl TestTool {
    fn read_only() -> Self {
        Self {
            name: "test_read",
            side_effect: ToolSideEffect::ReadOnly,
            confirmation: ToolConfirmation::Automatic,
            invoked: Arc::new(AtomicBool::new(false)),
        }
    }

    fn mutation(invoked: Arc<AtomicBool>) -> Self {
        Self {
            name: "test_mutation",
            side_effect: ToolSideEffect::ProposesMutation,
            confirmation: ToolConfirmation::Required,
            invoked,
        }
    }
}

#[async_trait]
impl ConversationTool for TestTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: self.name.to_string(),
            version: 1,
            kind: CapabilityKind::Native,
            stage: CapabilityStage::Research,
            display_name: "Test tool".to_string(),
            description: "test tool".to_string(),
            artifact_type: "test_output".to_string(),
            input_schema: json!({ "type": "object" }),
            output_schema: json!({
                "type": "object",
                "required": ["echo"],
                "properties": { "echo": { "type": "object" } }
            }),
            context: Vec::new(),
            model: None,
            max_steps: 1,
            tools: Vec::new(),
            skills: Vec::new(),
            surfaces: vec![CapabilitySurface::Conversation],
            subjects: vec![CapabilitySubjectKind::Company],
            triggers: Vec::new(),
            manifest_hash: format!("test:{}:1", self.name),
            side_effect: self.side_effect,
            confirmation: self.confirmation,
            cache_policy: ToolCachePolicy::None,
            storage_policy: ToolStoragePolicy::MetadataOnly,
            timeout: Duration::from_secs(1),
            initial_activity: "test_running".to_string(),
        }
    }

    fn agent_input_schema(&self) -> Option<Value> {
        Some(json!({ "type": "object" }))
    }

    async fn invoke(
        &self,
        arguments: Value,
        _context: CapabilityExecutionContext,
        progress: mpsc::UnboundedSender<ToolProgress>,
    ) -> Result<ToolOutput, ToolExecutionError> {
        self.invoked.store(true, Ordering::SeqCst);
        let _ = progress.send(ToolProgress::activity("test_progress"));
        Ok(ToolOutput {
            artifact_type: "test_output".to_string(),
            payload: json!({ "echo": arguments }),
            sources: Vec::new(),
            warning: None,
            model_route: None,
            model_routes: Vec::new(),
            execution_steps: 1,
            agent_trace: Vec::new(),
        })
    }
}

fn drain_events(
    receiver: &mut mpsc::UnboundedReceiver<ToolLifecycleEvent>,
) -> Vec<ToolLifecycleEvent> {
    let mut events = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        events.push(event);
    }
    events
}

fn planning_descriptor(name: &str, kind: CapabilityKind, triggers: &[&str]) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_string(),
        version: 1,
        kind,
        stage: if kind == CapabilityKind::Agent {
            CapabilityStage::Challenge
        } else {
            CapabilityStage::Analysis
        },
        display_name: name.to_string(),
        description: "test planning capability".to_string(),
        artifact_type: "test_analysis".to_string(),
        input_schema: json!({ "type": "object" }),
        output_schema: json!({ "type": "object" }),
        context: vec![CapabilityContextKey::Subject],
        model: Some(TaskComplexity::Deep),
        max_steps: 1,
        tools: Vec::new(),
        skills: Vec::new(),
        surfaces: vec![CapabilitySurface::Conversation],
        subjects: vec![CapabilitySubjectKind::Company],
        triggers: triggers.iter().map(|value| (*value).to_string()).collect(),
        manifest_hash: format!("test:{name}:1"),
        side_effect: ToolSideEffect::ReadOnly,
        confirmation: ToolConfirmation::Automatic,
        cache_policy: ToolCachePolicy::None,
        storage_policy: ToolStoragePolicy::StructuredArtifact,
        timeout: Duration::from_secs(60),
        initial_activity: "test_planning".to_string(),
    }
}

fn mock_ai_runtime() -> AiRuntime {
    AiRuntime::new(
        AiSettings {
            provider: AiProviderKind::Mock,
            provider_chain: vec![AiProviderKind::Mock],
            openai_api_key: None,
            openai_base_url: "http://localhost".to_string(),
            openai_model: "mock".to_string(),
            openai_models: ModelTierSettings {
                simple: "mock".to_string(),
                standard: "mock".to_string(),
                deep: "mock".to_string(),
            },
            cli: CliSettings {
                provider: CliProviderKind::Codex,
                path: "codex".to_string(),
                model: Some("mock".to_string()),
                profile: None,
            },
            cli_models: ModelTierSettings {
                simple: "mock".to_string(),
                standard: "mock".to_string(),
                deep: "mock".to_string(),
            },
        },
        "/tmp/prudentia-capability-test.env",
    )
}

fn company_context(user_message: &str) -> ConversationContext {
    ConversationContext {
        thread_title: "Tencent".to_string(),
        thread_summary: String::new(),
        turn_summaries: Vec::new(),
        subject: json!({ "kind": "company", "subject_key": "0700.HK", "label": "腾讯控股" }),
        user_message: user_message.to_string(),
        recent_messages: Vec::new(),
        portfolio_summary: PortfolioSummary {
            total_market_value: 0.0,
            total_cost: 0.0,
            total_unrealized_pnl: 0.0,
            positions_count: 0,
            price_stale_count: 0,
            top_positions: Vec::new(),
            sectors: Vec::new(),
            market_groups: Vec::new(),
            base_currency: "CNY".to_string(),
            total_market_value_base: 0.0,
            total_cost_base: 0.0,
            total_unrealized_pnl_base: 0.0,
            fx_rates: Vec::new(),
            fx_stale_count: 0,
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        },
        portfolio_positions: Vec::new(),
        company_view: Some(json!({ "symbol": "0700.HK" })),
        recent_trades: Vec::new(),
        investment_system: json!({}),
        attachments: Vec::new(),
        research_sources: Vec::new(),
        research_warning: None,
        capability_artifacts: Vec::new(),
        subject_clarification: None,
        used_context: Vec::new(),
    }
}
