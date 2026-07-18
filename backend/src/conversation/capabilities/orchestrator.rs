use std::{sync::Arc, time::Instant};

use futures_util::future::join_all;
use tokio::sync::mpsc;

use crate::{
    conversation::engine::TurnCancellation,
    error::{AppError, AppResult},
};

use super::{
    public_tool_error, registry::ToolRegistry, CapabilityExecutionContext, CompletedToolCall,
    FailedToolCall, PlannedToolCall, ToolExecutionReport, ToolLifecycleEvent, ToolOutput, ToolPlan,
};

pub(super) struct ToolOrchestrator {
    registry: Arc<ToolRegistry>,
}

impl ToolOrchestrator {
    pub(super) fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }

    pub(super) async fn execute(
        &self,
        plan: ToolPlan,
        context: CapabilityExecutionContext,
        cancellation: &TurnCancellation,
        events: mpsc::UnboundedSender<ToolLifecycleEvent>,
    ) -> AppResult<ToolExecutionReport> {
        let calls = plan.into_calls();
        let total_steps = calls.len();
        let executions = calls
            .into_iter()
            .enumerate()
            .map(|(index, call)| {
                self.execute_call(
                    call,
                    index + 1,
                    total_steps,
                    context.clone(),
                    cancellation,
                    events.clone(),
                )
            })
            .collect::<Vec<_>>();
        let mut report = ToolExecutionReport::default();
        for outcome in join_all(executions).await {
            match outcome? {
                CallOutcome::Completed(completed) => report.completed.push(completed),
                CallOutcome::Failed(failed) => report.failures.push(failed),
            }
        }
        Ok(report)
    }

    async fn execute_call(
        &self,
        call: PlannedToolCall,
        step_index: usize,
        total_steps: usize,
        context: CapabilityExecutionContext,
        cancellation: &TurnCancellation,
        events: mpsc::UnboundedSender<ToolLifecycleEvent>,
    ) -> AppResult<CallOutcome> {
        cancellation.ensure_active()?;
        let descriptor = match self.registry.descriptor(&call) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                let (public_code, public_message) = public_tool_error(error.code());
                tracing::warn!(
                    call_id = %call.call_id,
                    tool_name = %call.tool_name,
                    code = %error.code(),
                    error = %error,
                    "conversation capability lookup failed"
                );
                send_event(
                    &events,
                    ToolLifecycleEvent::Failed {
                        call_id: call.call_id.clone(),
                        tool_name: call.tool_name.clone(),
                        tool_version: call.tool_version,
                        capability_kind: None,
                        display_name: None,
                        stage: call.stage,
                        step_index,
                        total_steps,
                        duration_ms: 0,
                        code: public_code.to_string(),
                        message: public_message.to_string(),
                    },
                )?;
                return Ok(CallOutcome::Failed(FailedToolCall {
                    call_id: call.call_id,
                    tool_name: call.tool_name,
                    tool_version: call.tool_version,
                    capability_kind: None,
                    display_name: None,
                    subject_label: call.subject_label.clone(),
                    manifest_hash: None,
                    duration_ms: 0,
                    storage_policy: None,
                    code: public_code.to_string(),
                    message: public_message.to_string(),
                }));
            }
        };
        send_event(
            &events,
            ToolLifecycleEvent::Started {
                call_id: call.call_id.clone(),
                tool_name: call.tool_name.clone(),
                tool_version: call.tool_version,
                capability_kind: descriptor.kind,
                display_name: descriptor.display_name.clone(),
                stage: call.stage,
                step_index,
                total_steps,
                activity: descriptor.initial_activity.clone(),
                subject_label: call.subject_label.clone(),
                cache_policy: descriptor.cache_policy,
                storage_policy: descriptor.storage_policy,
            },
        )?;

        let started = Instant::now();
        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
        let execution = self.registry.execute(&call, context, progress_tx);
        tokio::pin!(execution);
        let mut progress_open = true;
        let result = loop {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => {
                    return Err(AppError::internal("conversation run canceled"));
                }
                progress = progress_rx.recv(), if progress_open => {
                    match progress {
                        Some(progress) => send_event(
                            &events,
                            ToolLifecycleEvent::Progress {
                                call_id: call.call_id.clone(),
                                tool_name: call.tool_name.clone(),
                                tool_version: call.tool_version,
                                capability_kind: descriptor.kind,
                                display_name: descriptor.display_name.clone(),
                                stage: call.stage,
                                step_index,
                                total_steps,
                                activity: progress.activity,
                                detail: progress.detail,
                                subject_label: call.subject_label.clone(),
                            },
                        )?,
                        None => progress_open = false,
                    }
                }
                result = &mut execution => break result,
            }
        };
        while let Ok(progress) = progress_rx.try_recv() {
            send_event(
                &events,
                ToolLifecycleEvent::Progress {
                    call_id: call.call_id.clone(),
                    tool_name: call.tool_name.clone(),
                    tool_version: call.tool_version,
                    capability_kind: descriptor.kind,
                    display_name: descriptor.display_name.clone(),
                    stage: call.stage,
                    step_index,
                    total_steps,
                    activity: progress.activity,
                    detail: progress.detail,
                    subject_label: call.subject_label.clone(),
                },
            )?;
        }

        let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        match result {
            Ok(output) => {
                let (source_count, warning) = output_summary(&output);
                send_event(
                    &events,
                    ToolLifecycleEvent::Completed {
                        call_id: call.call_id.clone(),
                        tool_name: call.tool_name.clone(),
                        tool_version: call.tool_version,
                        capability_kind: descriptor.kind,
                        display_name: descriptor.display_name.clone(),
                        stage: call.stage,
                        step_index,
                        total_steps,
                        duration_ms,
                        source_count,
                        warning,
                    },
                )?;
                Ok(CallOutcome::Completed(CompletedToolCall {
                    call_id: call.call_id.clone(),
                    tool_name: call.tool_name.clone(),
                    tool_version: call.tool_version,
                    capability_kind: descriptor.kind,
                    display_name: descriptor.display_name,
                    subject_label: call.subject_label.clone(),
                    manifest_hash: descriptor.manifest_hash,
                    duration_ms,
                    storage_policy: descriptor.storage_policy,
                    output,
                }))
            }
            Err(error) => {
                let (public_code, public_message) = public_tool_error(error.code());
                tracing::warn!(
                    call_id = %call.call_id,
                    tool_name = %call.tool_name,
                    code = %error.code(),
                    error = %error,
                    "conversation capability execution failed"
                );
                send_event(
                    &events,
                    ToolLifecycleEvent::Failed {
                        call_id: call.call_id.clone(),
                        tool_name: call.tool_name.clone(),
                        tool_version: call.tool_version,
                        capability_kind: Some(descriptor.kind),
                        display_name: Some(descriptor.display_name.clone()),
                        stage: call.stage,
                        step_index,
                        total_steps,
                        duration_ms,
                        code: public_code.to_string(),
                        message: public_message.to_string(),
                    },
                )?;
                Ok(CallOutcome::Failed(FailedToolCall {
                    call_id: call.call_id.clone(),
                    tool_name: call.tool_name.clone(),
                    tool_version: call.tool_version,
                    capability_kind: Some(descriptor.kind),
                    display_name: Some(descriptor.display_name),
                    subject_label: call.subject_label.clone(),
                    manifest_hash: Some(descriptor.manifest_hash),
                    duration_ms,
                    storage_policy: Some(descriptor.storage_policy),
                    code: public_code.to_string(),
                    message: public_message.to_string(),
                }))
            }
        }
    }
}

enum CallOutcome {
    Completed(CompletedToolCall),
    Failed(FailedToolCall),
}

fn send_event(
    events: &mpsc::UnboundedSender<ToolLifecycleEvent>,
    event: ToolLifecycleEvent,
) -> AppResult<()> {
    events
        .send(event)
        .map_err(|_| AppError::internal("conversation tool event receiver closed"))
}

fn output_summary(output: &ToolOutput) -> (usize, bool) {
    (output.sources.len(), output.warning.is_some())
}
