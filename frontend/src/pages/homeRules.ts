import type {
  ActiveCapabilityCall,
  ConversationExecutionPlan,
  ConversationActiveCapability,
  ConversationCapabilityArtifact,
  ConversationAction,
  ConversationRun,
  ConversationRunPhase,
  MemoThreadMessage,
  MemoThreadSummary,
  PortfolioPosition,
  RunEvent,
  TaskRouteReason
} from "../types/domain";
import type { TranslationKey } from "../i18n";

export interface LiveConversationRun extends ConversationRun {
  streamContent: string;
  messageId?: string;
  providerStage?: string;
  sourceCount?: number;
  toolName?: string;
  toolSubject?: string;
  toolStepIndex?: number;
  toolStepTotal?: number;
  activeCapabilities?: Record<string, ActiveCapabilityCall>;
  executionPlan?: ConversationExecutionPlan;
}

export function mergeStoredActiveRun(
  incoming: LiveConversationRun,
  existing?: LiveConversationRun
): LiveConversationRun {
  if (!existing || existing.id !== incoming.id) return incoming;
  if (Date.parse(existing.updated_at) > Date.parse(incoming.updated_at)) {
    if (existing.user_message_id) return existing;
    return {
      ...incoming,
      ...existing,
      client_request_id: incoming.client_request_id,
      user_message_id: incoming.user_message_id,
      assistant_message_id: incoming.assistant_message_id ?? existing.assistant_message_id,
      retry_of_run_id: incoming.retry_of_run_id ?? existing.retry_of_run_id,
      started_at: incoming.started_at,
      activeCapabilities: existing.activeCapabilities ?? incoming.activeCapabilities ?? {},
      executionPlan: existing.executionPlan ?? incoming.executionPlan
    };
  }
  return {
    ...incoming,
    streamContent: existing.streamContent,
    messageId: incoming.messageId ?? existing.messageId,
    providerStage: incoming.providerStage ?? existing.providerStage,
    sourceCount: incoming.sourceCount ?? existing.sourceCount,
    toolName: incoming.toolName ?? existing.toolName,
    toolSubject: incoming.toolSubject ?? existing.toolSubject,
    toolStepIndex: incoming.toolStepIndex ?? existing.toolStepIndex,
    toolStepTotal: incoming.toolStepTotal ?? existing.toolStepTotal,
    activeCapabilities: incoming.activeCapabilities ?? existing.activeCapabilities ?? {},
    executionPlan: incoming.executionPlan ?? existing.executionPlan
  };
}

export function activeCapabilitySnapshot(
  capabilities: ConversationActiveCapability[] = []
): Record<string, ActiveCapabilityCall> {
  return Object.fromEntries(capabilities.map((capability) => [
    capability.call_id,
    {
      callId: capability.call_id,
      capabilityId: capability.tool_name,
      capabilityKind: capability.capability_kind,
      displayName: capability.display_name,
      stage: capability.stage,
      activity: capability.activity,
      subject: capability.subject_label ?? undefined,
      stepIndex: capability.step_index,
      stepTotal: capability.total_steps,
      ...(capability.nested_tool_name ? { nestedToolName: capability.nested_tool_name } : {}),
      ...(capability.nested_tool_display_name
        ? { nestedToolDisplayName: capability.nested_tool_display_name }
        : {}),
      ...(capability.agent_turn !== undefined && capability.agent_turn !== null
        ? { agentTurn: capability.agent_turn }
        : {}),
      ...(capability.agent_turn_limit !== undefined && capability.agent_turn_limit !== null
        ? { agentTurnLimit: capability.agent_turn_limit }
        : {})
    }
  ]));
}

export interface ConstellationNode {
  id: string;
  symbol: string;
  label: string;
  group: string;
  radius: number;
  x: number;
  y: number;
  tone: string;
  color: string;
  weight: number;
}

export interface UsedContextDescriptor {
  key: TranslationKey;
  params: Record<string, string | number>;
}

export interface RunActivityDescriptor {
  key: TranslationKey;
  params: Record<string, string | number>;
}

export function taskComplexityKey(complexity?: string | null): TranslationKey | null {
  if (complexity === "simple") return "home.taskSimple";
  if (complexity === "standard") return "home.taskStandard";
  if (complexity === "deep") return "home.taskDeep";
  return null;
}

const routeReasonKeys: Record<TaskRouteReason, TranslationKey> = {
  social_turn: "home.routeReasonSocial",
  short_question: "home.routeReasonShort",
  subject_clarification: "home.routeReasonSubjectClarification",
  attachment_analysis: "home.routeReasonAttachment",
  investment_system: "home.routeReasonInvestmentSystem",
  multi_part_request: "home.routeReasonMultiPart",
  long_request: "home.routeReasonLong",
  explicit_deep_analysis: "home.routeReasonDeepAnalysis",
  company_research: "home.routeReasonCompanyResearch",
  standard_conversation: "home.routeReasonStandard"
};

export function parseTaskRouteReason(value: unknown): TaskRouteReason | undefined {
  return typeof value === "string" && value in routeReasonKeys
    ? value as TaskRouteReason
    : undefined;
}

export function applyConversationRunEvent(
  existing: LiveConversationRun,
  event: RunEvent
): LiveConversationRun {
  const next = { ...existing };
  if (event.event_type === "run.phase") {
    next.status = "running";
    next.phase = String(event.payload.phase ?? next.phase) as ConversationRunPhase;
    next.provider = typeof event.payload.provider === "string" ? event.payload.provider : next.provider;
    const detail = event.payload.detail && typeof event.payload.detail === "object"
      ? event.payload.detail as Record<string, unknown>
      : undefined;
    next.providerStage = typeof event.payload.provider_stage === "string"
      ? event.payload.provider_stage
      : typeof detail?.activity === "string"
        ? detail.activity
        : next.providerStage;
    next.sourceCount = typeof detail?.source_count === "number"
      ? detail.source_count
      : next.sourceCount;
  } else if (event.event_type.startsWith("tool.")) {
    next.status = "running";
    const stage = capabilityStage(event.payload.stage) ?? "research";
    next.phase = stage === "research" ? "researching" : "generating";
    next.toolName = stringValue(event.payload.tool_name) ?? next.toolName;
    next.toolSubject = stringValue(event.payload.subject_label) ?? next.toolSubject;
    next.toolStepIndex = numberValue(event.payload.step_index) ?? next.toolStepIndex;
    next.toolStepTotal = numberValue(event.payload.total_steps) ?? next.toolStepTotal;
    next.providerStage = stringValue(event.payload.activity) ?? next.providerStage;
    next.sourceCount = numberValue(event.payload.source_count) ?? next.sourceCount;
    const callId = stringValue(event.payload.call_id);
    if (callId) {
      const activeCapabilities = { ...(next.activeCapabilities ?? {}) };
      if (["tool.started", "tool.progress"].includes(event.event_type)) {
        const existingCall = activeCapabilities[callId];
        activeCapabilities[callId] = {
          callId,
          capabilityId:
            stringValue(event.payload.tool_name) ?? existingCall?.capabilityId ?? "capability",
          capabilityKind:
            capabilityKind(event.payload.capability_kind) ?? existingCall?.capabilityKind ?? "native",
          displayName:
            stringValue(event.payload.display_name) ?? existingCall?.displayName ?? next.toolName ?? "Capability",
          stage: stage ?? existingCall?.stage ?? "research",
          activity:
            stringValue(event.payload.activity) ?? existingCall?.activity ?? "capability_running",
          subject:
            stringValue(event.payload.subject_label) ?? existingCall?.subject,
          stepIndex:
            numberValue(event.payload.step_index) ?? existingCall?.stepIndex,
          stepTotal:
            numberValue(event.payload.total_steps) ?? existingCall?.stepTotal,
          nestedToolName: Object.hasOwn(event.payload, "nested_tool_name")
            ? stringValue(event.payload.nested_tool_name)
            : existingCall?.nestedToolName,
          nestedToolDisplayName: Object.hasOwn(event.payload, "nested_tool_display_name")
            ? stringValue(event.payload.nested_tool_display_name)
            : existingCall?.nestedToolDisplayName,
          agentTurn:
            numberValue(event.payload.agent_turn) ?? existingCall?.agentTurn,
          agentTurnLimit:
            numberValue(event.payload.agent_turn_limit) ?? existingCall?.agentTurnLimit
        };
      } else {
        delete activeCapabilities[callId];
      }
      next.activeCapabilities = activeCapabilities;
    }
  } else if (event.event_type === "run.plan.created") {
    next.executionPlan = executionPlanValue(event.payload);
  } else if (event.event_type === "run.plan.step") {
    const stepId = stringValue(event.payload.step_id);
    const status = stringValue(event.payload.status);
    if (next.executionPlan && stepId && status) {
      next.executionPlan = {
        ...next.executionPlan,
        steps: next.executionPlan.steps.map((step) =>
          step.id === stepId ? { ...step, status } : step
        )
      };
    }
  } else if (event.event_type === "run.classified" || event.event_type === "run.routed") {
    next.task_complexity = typeof event.payload.task_complexity === "string"
      ? event.payload.task_complexity
      : next.task_complexity;
    next.route_reason = parseTaskRouteReason(event.payload.route_reason) ?? next.route_reason;
    next.model = typeof event.payload.model === "string" ? event.payload.model : next.model;
    next.provider = typeof event.payload.provider === "string"
      ? event.payload.provider
      : next.provider;
  } else if (event.event_type === "message.delta") {
    next.streamContent += String(event.payload.content ?? "");
    next.messageId = stringValue(event.payload.message_id) ?? next.messageId;
  } else if (event.event_type === "message.completed") {
    if (typeof event.payload.content === "string") next.streamContent = event.payload.content;
    next.messageId = stringValue(event.payload.message_id) ?? next.messageId;
  } else if (event.event_type.startsWith("run.")) {
    const status = event.event_type.slice(4);
    if (["completed", "failed", "canceled", "interrupted"].includes(status)) {
      next.status = status as ConversationRun["status"];
      next.phase = status as ConversationRunPhase;
      next.error_message = stringValue(event.payload.message) ?? next.error_message;
      next.activeCapabilities = {};
      if (next.executionPlan && status !== "completed") {
        next.executionPlan = {
          ...next.executionPlan,
          steps: next.executionPlan.steps.map((step) => ({
            ...step,
            status: step.status === "running"
              ? "failed"
              : step.status === "pending"
                ? "skipped"
                : step.status
          }))
        };
      }
    }
  }
  next.updated_at = event.created_at;
  return next;
}

function executionPlanValue(payload: Record<string, unknown>): ConversationExecutionPlan | undefined {
  const templateId = stringValue(payload.template_id);
  const scope = stringValue(payload.scope);
  if (!templateId || !scope || !Array.isArray(payload.dimensions) || !Array.isArray(payload.steps)) {
    return undefined;
  }
  const steps = payload.steps.flatMap((value) => {
    if (!value || typeof value !== "object") return [];
    const step = value as Record<string, unknown>;
    const id = stringValue(step.id);
    const status = stringValue(step.status);
    return id && status ? [{ id, status }] : [];
  });
  return {
    template_id: templateId,
    scope,
    dimensions: payload.dimensions.filter((value): value is string => typeof value === "string"),
    steps
  };
}

const executionPlanScopeKeys: Record<string, TranslationKey> = {
  default: "home.planScopeDefault",
  business_model: "home.planScopeBusinessModel",
  moat: "home.planScopeMoat",
  earnings: "home.planScopeEarnings",
  news: "home.planScopeNews",
  risk: "home.planScopeRisk",
  fundamentals: "home.planScopeFundamentals"
};

const executionPlanDimensionKeys: Record<string, TranslationKey> = {
  business_model: "home.planDimensionBusinessModel",
  owner_economics: "home.planDimensionOwnerEconomics",
  competitive_position: "home.planDimensionCompetition",
  moat: "home.planDimensionMoat",
  management_capital_allocation: "home.planDimensionManagement",
  financial_resilience: "home.planDimensionResilience",
  earning_power: "home.planDimensionEarningPower",
  failure_mechanism: "home.planDimensionFailure"
};

const executionPlanStepKeys: Record<string, TranslationKey> = {
  scope: "home.planStepScope",
  research: "home.planStepResearch",
  evidence_baseline: "home.planStepEvidenceBaseline",
  analysis: "home.planStepAnalysis",
  challenge: "home.planStepChallenge",
  synthesis: "home.planStepSynthesis",
  memo_update: "home.planStepMemoUpdate"
};

export function executionPlanScopeKey(scope: string): TranslationKey {
  return executionPlanScopeKeys[scope] ?? "home.planScopeDefault";
}

export function executionPlanDimensionKey(dimension: string): TranslationKey {
  return executionPlanDimensionKeys[dimension] ?? "home.planDimensionUnknown";
}

export function executionPlanStepKey(step: string): TranslationKey {
  return executionPlanStepKeys[step] ?? "home.planStepUnknown";
}

export function executionPlanProgress(plan: ConversationExecutionPlan) {
  return {
    completed: plan.steps.filter((step) => step.status === "completed").length,
    total: plan.steps.length
  };
}

function stringValue(value: unknown) {
  return typeof value === "string" ? value : undefined;
}

function numberValue(value: unknown) {
  return typeof value === "number" ? value : undefined;
}

function capabilityKind(value: unknown): ActiveCapabilityCall["capabilityKind"] | undefined {
  return value === "native" || value === "skill" || value === "agent" ? value : undefined;
}

function capabilityStage(value: unknown): ActiveCapabilityCall["stage"] | undefined {
  return value === "research" || value === "analysis" || value === "challenge" ? value : undefined;
}

export function taskRouteReasonKey(reason?: string | null): TranslationKey | null {
  const parsed = parseTaskRouteReason(reason);
  return parsed ? routeReasonKeys[parsed] : null;
}

const constellationColors = ["#2f6f73", "#8b5d33", "#7a4a63", "#4f6f37", "#53617a", "#94624f"];

export function chatHomeDefaultThreadId(threads: MemoThreadSummary[]) {
  const activeThreads = threadRailItems(threads, 50);
  if (!activeThreads.length) {
    return null;
  }

  return activeThreads[0].id;
}

export function threadRailItems<T extends MemoThreadSummary>(threads: T[], limit = 12): T[] {
  return threads.filter((thread) => !thread.archived_at && thread.status !== "deleted").slice(0, limit);
}

export function memoChatElapsedSeconds(startedAtMs: number, nowMs: number) {
  return Math.max(0, Math.floor((nowMs - startedAtMs) / 1_000));
}

export function shouldScrollConversationToBottom({
  threadId,
  pinnedThreadId,
  messageCount,
  distanceFromBottom
}: {
  threadId: string | null;
  pinnedThreadId: string | null;
  messageCount: number;
  distanceFromBottom: number;
}) {
  if (!threadId || messageCount === 0) return false;
  return threadId !== pinnedThreadId || distanceFromBottom < 140;
}

export function runActivityDescriptor(run: {
  phase: ConversationRunPhase;
  providerStage?: string;
  sourceCount?: number;
  toolSubject?: string;
  nestedToolDisplayName?: string;
  agentTurn?: number;
  agentTurnLimit?: number;
}): RunActivityDescriptor {
  if (
    run.phase === "generating" &&
    run.providerStage === "provider_reading_context" &&
    (run.sourceCount ?? 0) > 0
  ) {
    return {
      key: "home.activityReadingSources",
      params: { count: run.sourceCount ?? 0 }
    };
  }

  const preparingCompanyKey: TranslationKey = run.toolSubject
    ? "home.activityPreparingCompanyResearch"
    : "home.activityPreparingCompanyResearchGeneric";
  const preparingCommunityKey: TranslationKey = run.toolSubject
    ? "home.activityPreparingCommunityInsights"
    : "home.activityPreparingCommunityInsightsGeneric";
  const researchActivityKeys: Record<string, TranslationKey> = {
    research_preparing_company: preparingCompanyKey,
    research_preparing_community_insights: preparingCommunityKey,
    research_checking_cache: "home.activityCheckingResearchCache",
    research_cache_hit: "home.activityResearchCacheHit",
    research_fetching_public_sources: "home.activityFetchingPublicSources",
    research_fetching_financial_history: "home.activityFetchingFinancialHistory",
    research_verifying_sources: "home.activityVerifyingSources",
    research_searching_official: "home.activitySearchingOfficial",
    research_searching_independent: "home.activitySearchingIndependent",
    research_searching_community: "home.activitySearchingCommunity"
  };
  const providerActivityKeys: Record<string, TranslationKey> = {
    provider_preparing: "home.activityPreparingContext",
    process_starting: "home.activityStartingProvider",
    provider_ready: "home.activityProviderReady",
    provider_reading_context: "home.activityReadingContext",
    provider_analyzing_evidence: "home.activityAnalyzingEvidence",
    provider_using_tool: "home.activityAdditionalStep",
    provider_writing_response: "home.activityWritingResponse",
    provider_completed: "home.activityFinalizingTurn",
    provider_failed: "home.runFailed",
    request_started: "home.activityStartingProvider",
    generating: "home.activityAnalyzingEvidence",
    provider_fallback: "home.activityProviderFallback"
  };
  const capabilityActivityKeys: Record<string, TranslationKey> = {
    skill_analyzing_business_model: "home.activitySkillBusinessModel",
    skill_auditing_moat: "home.activitySkillMoatAudit",
    agent_challenging_company_thesis: "home.activityAgentThesisChallenge",
    skill_applying_method: "home.activitySkillApplyingMethod",
    agent_forming_independent_view: "home.activityAgentIndependentView",
    agent_reviewing_its_analysis: "home.activityAgentReviewingAnalysis",
    agent_analyzing_company: "home.activityAgentAnalyzingCompany",
    agent_planning_next_step: "home.activityAgentPlanning",
    agent_calling_read_only_tool: "home.activityAgentCallingTool",
    agent_evaluating_tool_result: "home.activityAgentEvaluatingTool",
    agent_synthesizing_result: "home.activityAgentSynthesizing",
    capability_running: "home.activityCapabilityRunning"
  };
  const capabilityActivityKey = run.providerStage
    ? capabilityActivityKeys[run.providerStage]
    : undefined;
  if (capabilityActivityKey) {
    let params: Record<string, string | number> = {};
    if (run.providerStage === "agent_planning_next_step") {
      params = { turn: run.agentTurn ?? 1, total: run.agentTurnLimit ?? 1 };
    } else if (["agent_calling_read_only_tool", "agent_evaluating_tool_result"].includes(
      run.providerStage ?? ""
    )) {
      params = { tool: run.nestedToolDisplayName ?? "" };
    }
    return {
      key: capabilityActivityKey,
      params
    };
  }
  const activityKey = run.providerStage
    ? researchActivityKeys[run.providerStage]
      ?? (run.phase === "generating" ? providerActivityKeys[run.providerStage] : undefined)
    : undefined;
  if (activityKey) {
    const companyParamKeys: TranslationKey[] = [
      "home.activityPreparingCompanyResearch",
      "home.activityPreparingCommunityInsights"
    ];
    return {
      key: activityKey,
      params: companyParamKeys.includes(activityKey)
        ? { company: run.toolSubject ?? "" }
        : {}
    };
  }

  const phaseKeys: Record<ConversationRunPhase, TranslationKey> = {
    queued: "home.phaseQueued",
    resolving_subject: "home.phaseResolvingSubject",
    loading_context: "home.phaseLoadingContext",
    researching: "home.phaseResearching",
    generating: "home.phaseGenerating",
    extracting_actions: "home.phaseExtractingActions",
    persisting: "home.activityFinalizingTurn",
    completed: "home.activityResponseReady",
    failed: "home.runFailed",
    canceled: "home.runCanceled",
    interrupted: "home.runInterrupted"
  };
  return { key: phaseKeys[run.phase] ?? "home.backendRunning", params: {} };
}

export function activeCapabilityCalls(run: LiveConversationRun): ActiveCapabilityCall[] {
  return Object.values(run.activeCapabilities ?? {}).sort((left, right) => {
    const stepDifference = (left.stepIndex ?? 0) - (right.stepIndex ?? 0);
    return stepDifference || left.callId.localeCompare(right.callId);
  });
}

export function conversationCapabilityArtifacts(
  artifacts: unknown[]
): ConversationCapabilityArtifact[] {
  return artifacts.flatMap((artifact) => {
    if (!artifact || typeof artifact !== "object") return [];
    const value = artifact as Record<string, unknown>;
    const kind = value.capability_kind;
    const payload = value.payload;
    if (
      (kind !== "skill" && kind !== "agent") ||
      !payload ||
      typeof payload !== "object" ||
      Array.isArray(payload)
    ) {
      return [];
    }
    const capabilityId = stringValue(value.capability_id);
    const callId = stringValue(value.call_id);
    if (!capabilityId || !callId) return [];
    return [{
      call_id: callId,
      capability_id: capabilityId,
      capability_version: numberValue(value.capability_version) ?? 1,
      capability_kind: kind,
      display_name: stringValue(value.display_name) ?? capabilityId,
      artifact_type: stringValue(value.artifact_type) ?? "structured_analysis",
      status: value.status === "failed" ? "failed" : "completed",
      subject_label: stringValue(value.subject_label),
      payload: payload as Record<string, unknown>,
      source_ids: stringArray(value.source_ids),
      warning: stringValue(value.warning),
      error_code: stringValue(value.error_code),
      error_message: stringValue(value.error_message),
      provider: stringValue(value.provider),
      model: stringValue(value.model),
      model_steps: modelSteps(value.model_steps),
      manifest_hash: stringValue(value.manifest_hash) ?? "",
      duration_ms: numberValue(value.duration_ms) ?? 0,
      execution_steps: numberValue(value.execution_steps) ?? 1,
      agent_trace: agentTrace(value.agent_trace)
    }];
  });
}

function agentTrace(value: unknown): ConversationCapabilityArtifact["agent_trace"] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const entry = item as Record<string, unknown>;
    const turn = numberValue(entry.turn);
    const action = stringValue(entry.action);
    const status = stringValue(entry.status);
    if (turn === undefined || !action || !status) return [];
    return [{
      turn,
      action,
      tool_id: stringValue(entry.tool_id),
      tool_version: numberValue(entry.tool_version),
      tool_display_name: stringValue(entry.tool_display_name),
      status,
      source_count: numberValue(entry.source_count) ?? 0,
      error_code: stringValue(entry.error_code)
    }];
  });
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : [];
}

function modelSteps(value: unknown) {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const step = item as Record<string, unknown>;
    const index = numberValue(step.step);
    const provider = stringValue(step.provider);
    const model = stringValue(step.model);
    return index === undefined || !provider || !model
      ? []
      : [{ step: index, provider, model }];
  });
}

export function shouldSubmitComposerMessage(event: {
  key: string;
  shiftKey: boolean;
  isComposing: boolean;
  keyCode: number;
}) {
  return (
    event.key === "Enter" &&
    !event.shiftKey &&
    !event.isComposing &&
    event.keyCode !== 229
  );
}

export function usedContextDescriptor(item: unknown): UsedContextDescriptor {
  if (!item || typeof item !== "object") {
    return {
      key: "home.contextRaw",
      params: { value: String(item ?? "") }
    };
  }

  const value = item as Record<string, unknown>;
  const kind = typeof value.kind === "string" ? value.kind : "";
  const label = String(value.label ?? kind);
  const count = firstInteger(label);

  switch (kind) {
    case "thread_summary":
      return { key: "home.contextThreadSummary", params: { value: label } };
    case "turn_summaries":
      return { key: "home.contextPriorTurns", params: { count } };
    case "portfolio":
      return { key: "home.contextPositions", params: { count } };
    case "investment_system":
      return { key: "home.contextRuleGraph", params: { version: count } };
    case "company":
      return { key: "home.contextCompanyView", params: { value: label } };
    case "attachment":
      return { key: "home.contextAttachment", params: { value: label } };
    case "source":
      return { key: "home.contextResearchSource", params: { value: label } };
    default:
      return { key: "home.contextRaw", params: { value: label } };
  }
}

function firstInteger(value: string) {
  const match = value.match(/\d+/);
  return match ? Number(match[0]) : 0;
}

export function mergeConversationMessages(
  remote: MemoThreadMessage[],
  optimistic: MemoThreadMessage[],
  run?: LiveConversationRun
) {
  const messages = remote.map((message) => {
    const isStreamedAssistant =
      message.role === "assistant" &&
      (message.id === run?.messageId || message.request_id === run?.client_request_id);
    if (!run?.streamContent || !isStreamedAssistant) return message;
    return {
      ...message,
      content: run.streamContent,
      status: run.status === "running" ? ("streaming" as const) : message.status
    };
  });
  optimistic.forEach((message) => {
    if (
      !messages.some(
        (remoteMessage) =>
          remoteMessage.role === message.role && remoteMessage.content === message.content
      )
    ) {
      messages.push(message);
    }
  });
  if (
    run?.streamContent &&
    !messages.some(
      (message) =>
        message.role === "assistant" &&
        (message.id === run.messageId || message.request_id === run.client_request_id)
    )
  ) {
    messages.push({
      id: run.messageId ?? `live:${run.id}`,
      thread_id: run.thread_id,
      role: "assistant",
      content: run.streamContent,
      status:
        run.status === "running"
          ? "streaming"
          : run.status === "canceled"
            ? "canceled"
            : run.status === "failed" || run.status === "interrupted"
              ? "failed"
              : "completed",
      request_id: run.client_request_id,
      duration_ms: null,
      artifacts: [],
      sources: [],
      used_context: [],
      created_at: run.started_at,
      updated_at: run.updated_at
    });
  }
  return messages;
}

export function placeConversationActions(
  messages: MemoThreadMessage[],
  actions: ConversationAction[]
) {
  const visibleMessageIds = new Set(messages.map((message) => message.id));
  const byMessageId: Record<string, ConversationAction[]> = {};
  const unplacedActive: ConversationAction[] = [];

  actions.forEach((action) => {
    const messageId = action.assistant_message_id;
    if (messageId && visibleMessageIds.has(messageId)) {
      byMessageId[messageId] = [...(byMessageId[messageId] ?? []), action];
    } else if (!["executed", "rejected"].includes(action.status)) {
      unplacedActive.push(action);
    }
  });

  return { byMessageId, unplacedActive };
}

export function constellationNodes(positions: PortfolioPosition[]) {
  const groups = new Map<string, number>();
  return positions.map((position, index) => {
    const group = `${position.market || "Other"}/${position.currency || "N/A"}`;
    if (!groups.has(group)) {
      groups.set(group, groups.size);
    }
    const groupIndex = groups.get(group) ?? 0;
    const angle = -Math.PI / 2 + index * 1.92 + groupIndex * 0.34;
    const lane = 116 + (index % 3) * 34 + groupIndex * 14;
    const weight = Math.max(0, position.weight || 0);

    return {
      id: position.symbol,
      symbol: position.symbol,
      label: position.name || position.symbol,
      group,
      radius: Math.max(9, Math.round(12 + Math.sqrt(weight) * 38)),
      x: Math.round(250 + Math.cos(angle) * lane),
      y: Math.round(190 + Math.sin(angle) * lane * 0.74),
      tone: group,
      color: constellationColors[groupIndex % constellationColors.length],
      weight
    };
  });
}

export function shortThreadTitle(title: string) {
  const trimmed = title.trim();
  if (trimmed.length <= 28) {
    return trimmed || "Untitled";
  }
  return `${trimmed.slice(0, 28)}...`;
}

export function formatThreadTime(value: string, locale: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "";
  }

  return new Intl.DateTimeFormat(locale, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit"
  }).format(date);
}
