use std::sync::Arc;

use serde_json::{json, Value};
use tokio::{sync::mpsc, time::timeout};

use crate::{
    ai::{AiError, AiProviderEvent, ConversationProjection},
    error::{AppError, AppResult},
    locale::Locale,
};

use super::{
    super::{
        actions,
        capabilities::{CapabilityExecutionContext, CapabilityStage, ToolLifecycleEvent, ToolPlan},
        context::{
            assemble_context, assemble_subject_clarification_context, ConversationResearchContext,
        },
        execution_plan::build_company_execution_plan,
        storage,
        subject_resolution::resolve_turn_subject,
        task_routing::{assess_task, subject_clarification_assessment},
        types::{ConversationRun, RunEvent},
    },
    events::ConversationEvent,
    task::{TurnCancellation, TurnTask},
    turn_context::{StepContext, TurnContext},
    turn_support::{
        action_projection_complexity, action_projection_timeout, action_type_allowed_for_subject,
        casual_turn_summary, enrich_draft, fallback_summary, finish_visible_response,
        response_timeout, should_skip_action_projection,
    },
    ConversationEngine,
};

mod tool_runtime;

use tool_runtime::TurnArtifacts;

impl ConversationEngine {
    pub(super) fn spawn(self: &Arc<Self>, run_id: String, locale: Locale) {
        let engine = self.clone();
        let task_run_id = run_id.clone();
        let (start_tx, start_rx) = tokio::sync::oneshot::channel();
        let task = TurnTask::spawn(move |cancellation| async move {
            let _ = start_rx.await;
            if let Err(error) = engine.run_turn(&task_run_id, locale, &cancellation).await {
                if !cancellation.is_cancelled() {
                    tracing::error!(run_id = %task_run_id, %error, "conversation run failed");
                    let _ = engine.fail_run(&task_run_id, "conversation_failed").await;
                }
            }
            engine
                .tasks
                .lock()
                .expect("conversation task lock poisoned")
                .remove(&task_run_id);
        });
        self.tasks
            .lock()
            .expect("conversation task lock poisoned")
            .insert(run_id, task);
        let _ = start_tx.send(());
    }

    async fn run_turn(
        &self,
        run_id: &str,
        locale: Locale,
        cancellation: &TurnCancellation,
    ) -> AppResult<()> {
        let turn = self
            .resolve_turn_context(run_id, locale, cancellation)
            .await?;
        let research_plan = turn.tool_plan.for_stage(&[CapabilityStage::Research]);
        let research_planned = research_plan.has_calls();
        if research_planned {
            self.update_plan_step(&turn, "research", "running").await?;
        }
        let research = self
            .execute_tools(
                &turn,
                research_plan,
                CapabilityExecutionContext::without_conversation(turn.locale),
                cancellation,
            )
            .await?;
        if research_planned {
            let status = if research.had_failures {
                "failed"
            } else {
                "completed"
            };
            self.update_plan_step(&turn, "research", status).await?;
        }
        self.update_plan_step(&turn, "evidence_baseline", "running")
            .await?;
        let mut step = self
            .assemble_step_context(turn, research, cancellation)
            .await?;
        self.update_plan_step(&step.turn, "evidence_baseline", "completed")
            .await?;
        let mut analysis_ran = false;
        let mut challenge_ran = false;
        if step.turn.clarification.is_none() {
            let analysis_plan = step
                .turn
                .tool_plan
                .for_stage(&[CapabilityStage::Analysis, CapabilityStage::Challenge]);
            if analysis_plan.has_calls() {
                analysis_ran = analysis_plan.has_stage(CapabilityStage::Analysis);
                challenge_ran = analysis_plan.has_stage(CapabilityStage::Challenge);
                if analysis_ran {
                    self.update_plan_step(&step.turn, "analysis", "running")
                        .await?;
                }
                if challenge_ran {
                    self.update_plan_step(&step.turn, "challenge", "running")
                        .await?;
                }
                let analysis = self
                    .execute_tools(
                        &step.turn,
                        analysis_plan,
                        CapabilityExecutionContext::with_conversation(
                            step.turn.locale,
                            Arc::new(step.model.clone()),
                        ),
                        cancellation,
                    )
                    .await?;
                step.absorb(analysis);
                if analysis_ran {
                    self.update_plan_step(&step.turn, "analysis", "completed")
                        .await?;
                }
                if challenge_ran {
                    self.update_plan_step(&step.turn, "challenge", "completed")
                        .await?;
                }
            }
        }
        if !analysis_ran {
            self.update_plan_step(&step.turn, "analysis", "running")
                .await?;
        }
        if !challenge_ran {
            self.update_plan_step(&step.turn, "challenge", "running")
                .await?;
        }
        self.update_plan_step(&step.turn, "synthesis", "running")
            .await?;
        let (assistant_response, saw_delta) = self.generate_response(&step, cancellation).await?;
        if !analysis_ran {
            self.update_plan_step(&step.turn, "analysis", "completed")
                .await?;
        }
        if !challenge_ran {
            self.update_plan_step(&step.turn, "challenge", "completed")
                .await?;
        }
        self.update_plan_step(&step.turn, "synthesis", "completed")
            .await?;
        cancellation.ensure_active()?;

        let source_payloads = step.source_payloads()?;
        let message_id = storage::complete_assistant_message(
            &self.pool,
            run_id,
            &assistant_response,
            &step.artifacts,
            &source_payloads,
            &step.model.used_context,
        )
        .await?;
        cancellation.ensure_active()?;
        self.emit(
            run_id,
            &step.turn.run.thread_id,
            ConversationEvent::MessageCompleted {
                message_id: message_id.clone(),
                content: (!saw_delta).then_some(assistant_response.clone()),
            },
        )
        .await?;

        if step.turn.clarification.is_some() {
            self.persist_subject_clarification(
                &step.turn.run,
                &step.turn.subject,
                step.turn.locale,
                &message_id,
                cancellation,
            )
            .await?;
            return Ok(());
        }

        self.update_plan_step(&step.turn, "memo_update", "running")
            .await?;
        self.project_and_persist_turn(&step, &assistant_response, &message_id, cancellation)
            .await
    }

    async fn resolve_turn_context(
        &self,
        run_id: &str,
        locale: Locale,
        cancellation: &TurnCancellation,
    ) -> AppResult<TurnContext> {
        cancellation.ensure_active()?;
        let run = storage::run_by_id(&self.pool, run_id).await?;
        let user_message = sqlx::query_scalar::<_, String>(
            "SELECT content FROM memo_thread_messages WHERE id = ?",
        )
        .bind(&run.user_message_id)
        .fetch_one(&self.pool)
        .await?;
        self.phase(&run, "resolving_subject", None, None).await?;
        cancellation.ensure_active()?;

        let (subject, clarification, effective_user_message) =
            resolve_turn_subject(&self.pool, &run.thread_id, &user_message).await?;
        cancellation.ensure_active()?;
        let tool_plan = if clarification.is_none() {
            self.tools
                .plan_turn(run_id, &effective_user_message, &subject)?
        } else {
            ToolPlan::empty()
        };
        let execution_plan = if clarification.is_none() {
            build_company_execution_plan(
                &effective_user_message,
                &subject,
                tool_plan.has_stage(CapabilityStage::Research),
            )
        } else {
            None
        };
        let assessment = if clarification.is_some() {
            subject_clarification_assessment()
        } else {
            assess_task(
                &effective_user_message,
                &subject,
                storage::run_has_attachments(&self.pool, run_id).await?,
                tool_plan.has_calls(),
            )
        };
        storage::set_run_task_assessment(
            &self.pool,
            run_id,
            assessment.complexity.as_str(),
            assessment.reason.as_str(),
        )
        .await?;
        cancellation.ensure_active()?;
        let classified_run = storage::run_by_id(&self.pool, run_id).await?;
        self.emit(
            run_id,
            &run.thread_id,
            ConversationEvent::RunClassified {
                run: classified_run,
                task_complexity: assessment.complexity.as_str().to_string(),
                route_reason: assessment.reason.as_str().to_string(),
            },
        )
        .await?;
        if let Some(plan) = &execution_plan {
            self.emit(
                run_id,
                &run.thread_id,
                ConversationEvent::RunPlanCreated { plan: plan.clone() },
            )
            .await?;
        }

        Ok(TurnContext {
            run,
            locale,
            user_message,
            effective_user_message,
            subject,
            clarification,
            task_complexity: assessment.complexity,
            route_reason: assessment.reason,
            tool_plan,
            execution_plan,
        })
    }

    async fn assemble_step_context(
        &self,
        turn: TurnContext,
        research: TurnArtifacts,
        cancellation: &TurnCancellation,
    ) -> AppResult<StepContext> {
        cancellation.ensure_active()?;
        self.phase(
            &turn.run,
            "loading_context",
            None,
            turn.clarification
                .as_ref()
                .map(|_| json!({ "activity": "subject_clarification" })),
        )
        .await?;
        let model = if let Some(clarification) = &turn.clarification {
            assemble_subject_clarification_context(&self.pool, &turn.user_message, clarification)
                .await?
        } else {
            assemble_context(
                &self.pool,
                &self.workspace_dir,
                &turn.run.id,
                &turn.run.thread_id,
                &turn.effective_user_message,
                &turn.subject,
                ConversationResearchContext {
                    sources: research.model_sources,
                    warning: research.warning,
                },
            )
            .await?
        };
        cancellation.ensure_active()?;
        Ok(StepContext {
            turn,
            model,
            sources: research.persisted_sources,
            artifacts: research.artifacts,
        })
    }

    async fn generate_response(
        &self,
        step: &StepContext,
        cancellation: &TurnCancellation,
    ) -> AppResult<(String, bool)> {
        self.phase(
            &step.turn.run,
            "generating",
            None,
            Some(json!({
                "activity": "provider_preparing",
                "source_count": step.model.research_sources.len()
            })),
        )
        .await?;
        cancellation.ensure_active()?;
        let (provider_tx, mut provider_rx) = mpsc::unbounded_channel();
        let visible_response_timeout = response_timeout(step.turn.task_complexity);
        let response = timeout(
            visible_response_timeout,
            self.ai.respond_to_conversation(
                &step.model,
                step.turn.locale,
                step.turn.task_complexity,
                step.turn.route_reason.as_str(),
                provider_tx,
            ),
        );
        tokio::pin!(response);
        let mut saw_delta = false;
        let mut provider_events_open = true;
        let assistant_response = loop {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => {
                    return Err(AppError::internal("conversation run canceled"));
                }
                event = provider_rx.recv(), if provider_events_open => {
                    match event {
                        Some(event) => {
                            saw_delta |= self.handle_provider_event(&step.turn.run, event).await?;
                        }
                        None => provider_events_open = false,
                    }
                }
                result = &mut response => {
                    break finish_visible_response(result, visible_response_timeout)?;
                },
            }
        };
        while let Ok(event) = provider_rx.try_recv() {
            cancellation.ensure_active()?;
            saw_delta |= self.handle_provider_event(&step.turn.run, event).await?;
        }
        Ok((assistant_response, saw_delta))
    }

    async fn project_and_persist_turn(
        &self,
        step: &StepContext,
        assistant_response: &str,
        message_id: &str,
        cancellation: &TurnCancellation,
    ) -> AppResult<()> {
        let turn = &step.turn;
        let projection = if should_skip_action_projection(
            &turn.effective_user_message,
            !step.model.attachments.is_empty(),
            !step.model.research_sources.is_empty(),
        ) {
            ConversationProjection {
                summary: casual_turn_summary(turn.locale).to_string(),
                actions: Vec::new(),
            }
        } else {
            self.phase(&turn.run, "extracting_actions", None, None)
                .await?;
            let projection_timeout = action_projection_timeout(turn.task_complexity);
            let projection = tokio::select! {
                biased;
                _ = cancellation.cancelled() => {
                    return Err(AppError::internal("conversation run canceled"));
                }
                result = timeout(
                    projection_timeout,
                    self.ai.project_conversation(
                        &step.model,
                        assistant_response,
                        turn.locale,
                        action_projection_complexity(&turn.subject),
                    ),
                ) => result,
            }
            .map_err(|_| {
                AiError::Provider(format!(
                    "action projection timed out after {} seconds",
                    projection_timeout.as_secs()
                ))
            })
            .and_then(|result| result);
            match projection {
                Ok(projection) => projection,
                Err(error) => {
                    self.emit(
                        &turn.run.id,
                        &turn.run.thread_id,
                        ConversationEvent::RunWarning {
                            code: "action_projection_failed".to_string(),
                            message: error.to_string(),
                        },
                    )
                    .await?;
                    ConversationProjection {
                        summary: fallback_summary(&turn.effective_user_message),
                        actions: Vec::new(),
                    }
                }
            }
        };

        cancellation.ensure_active()?;
        self.phase(&turn.run, "persisting", None, None).await?;
        storage::insert_turn_summary(
            &self.pool,
            &turn.run.id,
            &turn.run.thread_id,
            &turn.subject,
            &projection.summary,
        )
        .await?;
        for mut draft in projection.actions {
            cancellation.ensure_active()?;
            if !action_type_allowed_for_subject(&draft.action_type, &turn.subject) {
                self.emit(
                    &turn.run.id,
                    &turn.run.thread_id,
                    ConversationEvent::RunWarning {
                        code: "invalid_action_subject".to_string(),
                        message: format!(
                            "{} is not allowed for a {} subject",
                            draft.action_type, turn.subject.kind
                        ),
                    },
                )
                .await?;
                continue;
            }
            enrich_draft(&mut draft, &turn.subject);
            match actions::prepare_action(
                &self.pool,
                self.market_data.clone(),
                self.tools.rule_node_adapters(),
                draft,
            )
            .await
            {
                Ok((draft, target_version)) => {
                    cancellation.ensure_active()?;
                    let action = storage::insert_action(
                        &self.pool,
                        &turn.run.id,
                        &turn.run.thread_id,
                        draft,
                        target_version,
                    )
                    .await?;
                    self.emit(
                        &turn.run.id,
                        &turn.run.thread_id,
                        ConversationEvent::ActionProposed(action),
                    )
                    .await?;
                }
                Err(error) => {
                    self.emit(
                        &turn.run.id,
                        &turn.run.thread_id,
                        ConversationEvent::RunWarning {
                            code: "invalid_action_proposal".to_string(),
                            message: error.to_string(),
                        },
                    )
                    .await?;
                }
            }
        }
        cancellation.ensure_active()?;
        self.update_plan_step(turn, "memo_update", "completed")
            .await?;
        let transitioned = storage::finish_run(
            &self.pool,
            &turn.run.id,
            "completed",
            "completed",
            None,
            None,
        )
        .await?;
        if transitioned {
            self.emit(
                &turn.run.id,
                &turn.run.thread_id,
                ConversationEvent::RunCompleted {
                    message_id: message_id.to_string(),
                },
            )
            .await?;
        }
        Ok(())
    }

    async fn handle_provider_event(
        &self,
        run: &ConversationRun,
        event: AiProviderEvent,
    ) -> AppResult<bool> {
        match event {
            AiProviderEvent::RouteSelected {
                provider,
                model,
                complexity,
                reason,
            } => {
                storage::set_run_model_route(
                    &self.pool,
                    &run.id,
                    &provider,
                    &model,
                    &complexity,
                    &reason,
                )
                .await?;
                let routed_run = storage::run_by_id(&self.pool, &run.id).await?;
                self.emit(
                    &run.id,
                    &run.thread_id,
                    ConversationEvent::RunRouted {
                        run: routed_run,
                        provider,
                        model,
                        task_complexity: complexity,
                        route_reason: reason,
                    },
                )
                .await?;
                Ok(false)
            }
            AiProviderEvent::Stage { provider, stage } => {
                storage::set_run_phase(
                    &self.pool,
                    &run.id,
                    "generating",
                    Some(&provider),
                    Some(&stage),
                    None,
                )
                .await?;
                self.emit(
                    &run.id,
                    &run.thread_id,
                    ConversationEvent::ProviderPhase {
                        provider,
                        provider_stage: stage,
                    },
                )
                .await?;
                Ok(false)
            }
            AiProviderEvent::TextDelta(content) => {
                let message_id =
                    storage::append_assistant_delta(&self.pool, &run.id, &content).await?;
                self.emit(
                    &run.id,
                    &run.thread_id,
                    ConversationEvent::MessageDelta {
                        message_id,
                        content,
                    },
                )
                .await?;
                Ok(true)
            }
        }
    }

    async fn update_plan_step(
        &self,
        turn: &TurnContext,
        step_id: &str,
        status: &str,
    ) -> AppResult<()> {
        if turn.execution_plan.is_none() {
            return Ok(());
        }
        self.emit(
            &turn.run.id,
            &turn.run.thread_id,
            ConversationEvent::RunPlanStep {
                step_id: step_id.to_string(),
                status: status.to_string(),
            },
        )
        .await?;
        Ok(())
    }

    async fn handle_tool_lifecycle(
        &self,
        run: &ConversationRun,
        event: ToolLifecycleEvent,
    ) -> AppResult<()> {
        match &event {
            ToolLifecycleEvent::Started {
                activity, stage, ..
            }
            | ToolLifecycleEvent::Progress {
                activity, stage, ..
            } => {
                let phase = match stage {
                    CapabilityStage::Research => "researching",
                    CapabilityStage::Analysis | CapabilityStage::Challenge => "generating",
                };
                storage::set_run_phase(&self.pool, &run.id, phase, None, Some(activity), None)
                    .await?;
            }
            ToolLifecycleEvent::Completed {
                source_count,
                stage,
                ..
            } => {
                let source_count = i64::try_from(*source_count).unwrap_or(i64::MAX);
                let phase = match stage {
                    CapabilityStage::Research => "researching",
                    CapabilityStage::Analysis | CapabilityStage::Challenge => "generating",
                };
                storage::set_run_phase(&self.pool, &run.id, phase, None, None, Some(source_count))
                    .await?;
            }
            ToolLifecycleEvent::Failed { .. } => {}
        }
        self.emit(&run.id, &run.thread_id, ConversationEvent::Tool(event))
            .await?;
        Ok(())
    }

    pub(super) async fn phase(
        &self,
        run: &ConversationRun,
        phase: &str,
        provider: Option<&str>,
        detail: Option<Value>,
    ) -> AppResult<()> {
        let activity = detail
            .as_ref()
            .and_then(|value| value.get("activity"))
            .and_then(Value::as_str);
        let source_count = detail
            .as_ref()
            .and_then(|value| value.get("source_count"))
            .and_then(Value::as_i64);
        storage::set_run_phase(&self.pool, &run.id, phase, provider, activity, source_count)
            .await?;
        self.emit(
            &run.id,
            &run.thread_id,
            ConversationEvent::RunPhase {
                phase: phase.to_string(),
                provider: provider.map(str::to_string),
                detail,
            },
        )
        .await?;
        Ok(())
    }

    async fn fail_run(&self, run_id: &str, code: &str) -> AppResult<()> {
        const PUBLIC_MESSAGE: &str = "The response could not be completed. Retry the request.";
        let run = storage::run_by_id(&self.pool, run_id).await?;
        let transitioned = storage::finish_run(
            &self.pool,
            run_id,
            "failed",
            "failed",
            Some(code),
            Some(PUBLIC_MESSAGE),
        )
        .await?;
        if transitioned {
            storage::mark_assistant_terminal(&self.pool, run_id, "failed", None).await?;
            self.emit(
                run_id,
                &run.thread_id,
                ConversationEvent::RunFailed {
                    code: code.to_string(),
                    message: PUBLIC_MESSAGE.to_string(),
                    retryable: true,
                },
            )
            .await?;
        }
        Ok(())
    }

    pub(super) async fn emit(
        &self,
        run_id: &str,
        thread_id: &str,
        event: ConversationEvent,
    ) -> AppResult<RunEvent> {
        let (event_type, payload) = event.into_wire()?;
        let event =
            storage::append_event(&self.pool, run_id, thread_id, event_type, payload).await?;
        let _ = self.events.send(event.clone());
        Ok(event)
    }
}
