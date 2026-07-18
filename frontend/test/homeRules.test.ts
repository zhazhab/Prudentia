import assert from "node:assert/strict";
import test from "node:test";
import {
  activeCapabilitySnapshot,
  activeCapabilityCalls,
  applyConversationRunEvent,
  chatHomeDefaultThreadId,
  constellationNodes,
  conversationCapabilityArtifacts,
  executionPlanDimensionKey,
  executionPlanScopeKey,
  executionPlanStepKey,
  mergeStoredActiveRun,
  mergeConversationMessages,
  memoChatElapsedSeconds,
  placeConversationActions,
  runActivityDescriptor,
  shouldScrollConversationToBottom,
  shouldSubmitComposerMessage,
  taskComplexityKey,
  taskRouteReasonKey,
  threadRailItems,
  usedContextDescriptor
} from "../src/pages/homeRules.ts";
import type {
  ConversationAction,
  ConversationRun,
  MemoThreadMessage,
  MemoThreadSummary,
  PortfolioPosition,
  RunEvent
} from "../src/types/domain.ts";

test("chat home opens the latest active thread", () => {
  const threads = [
    thread({ id: "recent", title: "Recent" }),
    thread({ id: "last", title: "Last" })
  ];

  assert.equal(chatHomeDefaultThreadId(threads), "recent");
  assert.equal(chatHomeDefaultThreadId([]), null);
});

test("thread rail keeps the latest twelve active threads", () => {
  const threads = Array.from({ length: 14 }, (_, index) =>
    thread({
      id: `thread-${index}`,
      title: `Thread ${index}`,
      archived_at: index === 2 ? "2026-01-01T00:00:00Z" : null
    })
  );

  const items = threadRailItems(threads);

  assert.equal(items.length, 12);
  assert.equal(items.some((item) => item.id === "thread-2"), false);
  assert.equal(items[0].id, "thread-0");
});

test("memo chat runtime reports non-negative elapsed seconds", () => {
  assert.equal(memoChatElapsedSeconds(1_000, 1_000), 0);
  assert.equal(memoChatElapsedSeconds(1_000, 47_499), 46);
  assert.equal(memoChatElapsedSeconds(2_000, 1_000), 0);
});

test("conversation opens at the latest message without hijacking deliberate history scroll", () => {
  assert.equal(
    shouldScrollConversationToBottom({
      threadId: "thread-1",
      pinnedThreadId: null,
      messageCount: 25,
      distanceFromBottom: 9_000
    }),
    true
  );
  assert.equal(
    shouldScrollConversationToBottom({
      threadId: "thread-1",
      pinnedThreadId: "thread-1",
      messageCount: 25,
      distanceFromBottom: 9_000
    }),
    false
  );
  assert.equal(
    shouldScrollConversationToBottom({
      threadId: "thread-1",
      pinnedThreadId: "thread-1",
      messageCount: 26,
      distanceFromBottom: 80
    }),
    true
  );
});

test("run activity turns persisted backend stages into specific user-facing work", () => {
  assert.deepEqual(
    runActivityDescriptor({
      phase: "researching",
      providerStage: "research_preparing_community_insights",
      sourceCount: 0,
      toolSubject: "腾讯控股"
    }),
    { key: "home.activityPreparingCommunityInsights", params: { company: "腾讯控股" } }
  );
  assert.deepEqual(
    runActivityDescriptor({
      phase: "researching",
      providerStage: "research_preparing_company",
      sourceCount: 0,
      toolSubject: "腾讯控股"
    }),
    { key: "home.activityPreparingCompanyResearch", params: { company: "腾讯控股" } }
  );
  assert.deepEqual(
    runActivityDescriptor({
      phase: "researching",
      providerStage: "research_preparing_company",
      sourceCount: 0
    }),
    { key: "home.activityPreparingCompanyResearchGeneric", params: {} }
  );
  assert.deepEqual(
    runActivityDescriptor({
      phase: "researching",
      providerStage: "research_fetching_public_sources",
      sourceCount: 0
    }),
    { key: "home.activityFetchingPublicSources", params: {} }
  );
  assert.deepEqual(
    runActivityDescriptor({
      phase: "researching",
      providerStage: "research_fetching_financial_history",
      sourceCount: 0
    }),
    { key: "home.activityFetchingFinancialHistory", params: {} }
  );
  assert.deepEqual(
    runActivityDescriptor({
      phase: "generating",
      providerStage: "request_started",
      sourceCount: 0
    }),
    { key: "home.activityStartingProvider", params: {} }
  );
  assert.deepEqual(
    runActivityDescriptor({
      phase: "generating",
      providerStage: "provider_reading_context",
      sourceCount: 6
    }),
    { key: "home.activityReadingSources", params: { count: 6 } }
  );
  assert.deepEqual(
    runActivityDescriptor({
      phase: "generating",
      providerStage: "provider_writing_response",
      sourceCount: 6
    }),
    { key: "home.activityWritingResponse", params: {} }
  );
  assert.deepEqual(
    runActivityDescriptor({
      phase: "extracting_actions",
      providerStage: "provider_completed",
      sourceCount: 6
    }),
    { key: "home.phaseExtractingActions", params: {} }
  );
  assert.deepEqual(
    runActivityDescriptor({
      phase: "generating",
      providerStage: "provider_completed",
      sourceCount: 0
    }),
    { key: "home.activityFinalizingTurn", params: {} }
  );
  assert.deepEqual(
    runActivityDescriptor({
      phase: "persisting",
      providerStage: "provider_completed",
      sourceCount: 0
    }),
    { key: "home.activityFinalizingTurn", params: {} }
  );
});

test("durable tool events restore concrete research activity after reconnect", () => {
  const event: RunEvent = {
    event_id: 12,
    run_id: "run-1",
    thread_id: "thread-1",
    event_type: "tool.progress",
    payload: {
      call_id: "run-1:tool:1",
      tool_name: "research_company",
      step_index: 1,
      total_steps: 1,
      activity: "research_searching_official",
      subject_label: "腾讯控股"
    },
    created_at: "2026-01-01T00:00:03Z"
  };

  const run = applyConversationRunEvent(liveRun(), event);

  assert.equal(run.phase, "researching");
  assert.equal(run.providerStage, "research_searching_official");
  assert.equal(run.toolName, "research_company");
  assert.equal(run.toolSubject, "腾讯控股");
  assert.equal(run.toolStepIndex, 1);
  assert.equal(run.toolStepTotal, 1);
});

test("persisted run plan events expose scope, template, and live step status", () => {
  const created: RunEvent = {
    event_id: 18,
    run_id: "run-1",
    thread_id: "thread-1",
    event_type: "run.plan.created",
    payload: {
      template_id: "company_analysis_v1",
      scope: "moat",
      dimensions: ["business_model", "moat", "failure_mechanism"],
      steps: [
        { id: "scope", status: "completed" },
        { id: "research", status: "pending" }
      ]
    },
    created_at: "2026-01-01T00:00:03Z"
  };
  const progressing: RunEvent = {
    ...created,
    event_id: 19,
    event_type: "run.plan.step",
    payload: { step_id: "research", status: "running" },
    created_at: "2026-01-01T00:00:04Z"
  };

  const run = applyConversationRunEvent(
    applyConversationRunEvent(liveRun(), created),
    progressing
  );

  assert.equal(run.executionPlan?.scope, "moat");
  assert.equal(run.executionPlan?.steps[1].status, "running");
  assert.equal(executionPlanScopeKey("moat"), "home.planScopeMoat");
  assert.equal(executionPlanDimensionKey("failure_mechanism"), "home.planDimensionFailure");
  assert.equal(executionPlanStepKey("research"), "home.planStepResearch");
});

test("skill and agent lifecycle events restore parallel capability status", () => {
  const startedSkill: RunEvent = {
    event_id: 20,
    run_id: "run-1",
    thread_id: "thread-1",
    event_type: "tool.started",
    payload: {
      call_id: "run-1:tool:2",
      tool_name: "analyze_business_model",
      capability_kind: "skill",
      display_name: "商业模式分析",
      stage: "analysis",
      activity: "skill_analyzing_business_model",
      step_index: 2,
      total_steps: 3
    },
    created_at: "2026-01-01T00:00:04Z"
  };
  const startedAgent: RunEvent = {
    ...startedSkill,
    event_id: 21,
    event_type: "tool.progress",
    payload: {
      call_id: "run-1:tool:3",
      tool_name: "challenge_company_thesis",
      capability_kind: "agent",
      display_name: "反方分析",
      stage: "challenge",
      activity: "agent_forming_independent_view",
      step_index: 3,
      total_steps: 3
    }
  };

  const running = applyConversationRunEvent(
    applyConversationRunEvent(liveRun(), startedSkill),
    startedAgent
  );
  const active = activeCapabilityCalls(running);

  assert.equal(running.phase, "generating");
  assert.deepEqual(active.map((call) => call.capabilityKind), ["skill", "agent"]);
  assert.equal(active[0].activity, "skill_analyzing_business_model");
  assert.deepEqual(
    runActivityDescriptor({
      phase: "generating",
      providerStage: active[1].activity
    }),
    { key: "home.activityAgentIndependentView", params: {} }
  );

  const completed = applyConversationRunEvent(running, {
    ...startedSkill,
    event_id: 22,
    event_type: "tool.completed",
    payload: {
      call_id: "run-1:tool:2",
      tool_name: "analyze_business_model",
      capability_kind: "skill",
      display_name: "商业模式分析",
      stage: "analysis",
      duration_ms: 1234,
      source_count: 0
    }
  });
  assert.deepEqual(activeCapabilityCalls(completed).map((call) => call.capabilityKind), ["agent"]);
});

test("agent progress keeps the nested tool and turn visible after reconnect", () => {
  const event: RunEvent = {
    event_id: 23,
    run_id: "run-1",
    thread_id: "thread-1",
    event_type: "tool.progress",
    payload: {
      call_id: "run-1:tool:3",
      tool_name: "analyze_company",
      capability_kind: "agent",
      display_name: "公司深度分析",
      stage: "analysis",
      activity: "agent_calling_read_only_tool",
      nested_tool_name: "research_community_insights",
      nested_tool_display_name: "Community insights",
      agent_turn: 2,
      agent_turn_limit: 8,
      step_index: 2,
      total_steps: 2
    },
    created_at: "2026-01-01T00:00:05Z"
  };

  const running = applyConversationRunEvent(liveRun(), event);
  const active = activeCapabilityCalls(running)[0];

  assert.equal(active.nestedToolName, "research_community_insights");
  assert.equal(active.agentTurn, 2);
  assert.deepEqual(
    runActivityDescriptor({
      phase: "generating",
      providerStage: active.activity,
      nestedToolDisplayName: active.nestedToolDisplayName,
      agentTurn: active.agentTurn,
      agentTurnLimit: active.agentTurnLimit
    }),
    {
      key: "home.activityAgentCallingTool",
      params: { tool: "Community insights" }
    }
  );

  const planning = applyConversationRunEvent(running, {
    ...event,
    event_id: 24,
    payload: {
      ...event.payload,
      activity: "agent_planning_next_step",
      nested_tool_name: null,
      nested_tool_display_name: null,
      agent_turn: 3
    },
    created_at: "2026-01-01T00:00:06Z"
  });
  const planningCall = activeCapabilityCalls(planning)[0];
  assert.equal(planningCall.nestedToolName, undefined);
  assert.equal(planningCall.nestedToolDisplayName, undefined);
});

test("message artifacts expose only validated skill and agent outputs", () => {
  const parsed = conversationCapabilityArtifacts([
    { kind: "legacy_draft", payload: { ignored: true } },
    {
      call_id: "call-1",
      capability_id: "audit_moat",
      capability_version: 2,
      capability_kind: "skill",
      display_name: "护城河审计",
      artifact_type: "moat_audit",
      payload: { summary: "Causal audit", findings: [], open_questions: [] },
      source_ids: ["source-1"],
      provider: "cli",
      model: "deep-model",
      manifest_hash: "abc",
      duration_ms: 2500,
      execution_steps: 2,
      agent_trace: [{
        turn: 1,
        action: "tool",
        tool_id: "research_company",
        tool_version: 1,
        tool_display_name: "Company research",
        status: "completed",
        source_count: 6
      }]
    }
  ]);

  assert.equal(parsed.length, 1);
  assert.equal(parsed[0].capability_id, "audit_moat");
  assert.equal(parsed[0].payload.summary, "Causal audit");
  assert.deepEqual(parsed[0].source_ids, ["source-1"]);
  assert.equal(parsed[0].agent_trace?.[0].tool_id, "research_company");
});

test("persisted model routing metadata maps to explainable frontend copy", () => {
  assert.equal(taskComplexityKey("simple"), "home.taskSimple");
  assert.equal(taskComplexityKey("standard"), "home.taskStandard");
  assert.equal(taskComplexityKey("deep"), "home.taskDeep");
  assert.equal(taskComplexityKey("unknown"), null);
  assert.equal(taskRouteReasonKey("company_research"), "home.routeReasonCompanyResearch");
  assert.equal(
    taskRouteReasonKey("subject_clarification"),
    "home.routeReasonSubjectClarification"
  );
  assert.equal(taskRouteReasonKey("explicit_deep_analysis"), "home.routeReasonDeepAnalysis");
  assert.equal(taskRouteReasonKey("unknown"), null);
});

test("a stale active-run fetch cannot overwrite newer routed event state", () => {
  const stale = liveRun({
    updated_at: "2026-01-01T00:00:01Z",
    task_complexity: null,
    model: null,
    route_reason: null
  });
  const routed = liveRun({
    updated_at: "2026-01-01T00:00:02Z",
    task_complexity: "deep",
    model: "gpt-5.6-sol",
    route_reason: "explicit_deep_analysis",
    providerStage: "provider_analyzing_evidence"
  });

  assert.deepEqual(mergeStoredActiveRun(stale, routed), routed);
});

test("active capability snapshots restore unfinished parallel work after refresh", () => {
  const snapshot = activeCapabilitySnapshot([
    {
      call_id: "call-1",
      tool_name: "audit_moat",
      capability_kind: "skill",
      display_name: "Moat audit",
      stage: "model_call",
      activity: "Auditing moat evidence",
      subject_label: "Tencent",
      step_index: 2,
      total_steps: 3
    }
  ]);

  assert.deepEqual(snapshot, {
    "call-1": {
      callId: "call-1",
      capabilityId: "audit_moat",
      capabilityKind: "skill",
      displayName: "Moat audit",
      stage: "model_call",
      activity: "Auditing moat evidence",
      subject: "Tencent",
      stepIndex: 2,
      stepTotal: 3
    }
  });
});

test("REST hydration keeps a capability event received before the run snapshot", () => {
  const placeholder = liveRun({
    user_message_id: "",
    updated_at: "2026-01-01T00:00:02Z",
    activeCapabilities: {
      "call-live": {
        callId: "call-live",
        capabilityId: "challenge_company_thesis",
        capabilityKind: "agent",
        displayName: "Dissent analysis",
        stage: "model_call",
        activity: "Forming an independent view",
        stepIndex: 1,
        stepTotal: 2
      }
    }
  });
  const stored = liveRun({
    updated_at: "2026-01-01T00:00:01Z",
    activeCapabilities: {}
  });

  const merged = mergeStoredActiveRun(stored, placeholder);

  assert.equal(merged.user_message_id, "user-1");
  assert.ok(merged.activeCapabilities?.["call-live"]);
});

test("a newer REST snapshot removes capability calls that already finished", () => {
  const staleEventState = liveRun({
    updated_at: "2026-01-01T00:00:01Z",
    activeCapabilities: {
      "call-finished": {
        callId: "call-finished",
        capabilityId: "analyze_company",
        capabilityKind: "agent",
        displayName: "Company analysis",
        stage: "analysis",
        activity: "agent_synthesizing_result"
      }
    }
  });
  const newerStoredState = liveRun({
    updated_at: "2026-01-01T00:00:03Z",
    activeCapabilities: {}
  });

  const merged = mergeStoredActiveRun(newerStoredState, staleEventState);

  assert.deepEqual(merged.activeCapabilities, {});
});

test("an older REST snapshot cannot resurrect a call removed by a newer event", () => {
  const olderStoredState = liveRun({
    updated_at: "2026-01-01T00:00:01Z",
    activeCapabilities: {
      "call-finished": {
        callId: "call-finished",
        capabilityId: "analyze_company",
        capabilityKind: "agent",
        displayName: "Company analysis",
        stage: "analysis",
        activity: "agent_synthesizing_result"
      }
    }
  });
  const newerEventState = liveRun({
    updated_at: "2026-01-01T00:00:03Z",
    activeCapabilities: {}
  });

  const merged = mergeStoredActiveRun(olderStoredState, newerEventState);

  assert.deepEqual(merged.activeCapabilities, {});
});

test("IME confirmation Enter does not submit the conversation composer", () => {
  assert.equal(
    shouldSubmitComposerMessage({
      key: "Enter",
      shiftKey: false,
      isComposing: true,
      keyCode: 13
    }),
    false
  );
  assert.equal(
    shouldSubmitComposerMessage({
      key: "Enter",
      shiftKey: false,
      isComposing: false,
      keyCode: 229
    }),
    false
  );
  assert.equal(
    shouldSubmitComposerMessage({
      key: "Enter",
      shiftKey: true,
      isComposing: false,
      keyCode: 13
    }),
    false
  );
  assert.equal(
    shouldSubmitComposerMessage({
      key: "Enter",
      shiftKey: false,
      isComposing: false,
      keyCode: 13
    }),
    true
  );
});

test("streamed assistant content never overwrites the user message with the same request id", () => {
  const user = message({ id: "user-1", role: "user", content: "你好" });
  const assistant = message({ id: "assistant-1", role: "assistant", content: "旧回复" });
  const run: ConversationRun & { streamContent: string; messageId: string } = {
    id: "run-1",
    client_request_id: "request-1",
    thread_id: "thread-1",
    user_message_id: "user-1",
    assistant_message_id: "assistant-1",
    status: "completed",
    phase: "completed",
    provider: "cli",
    started_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:10Z",
    streamContent: "自然回复",
    messageId: "assistant-1"
  };

  const merged = mergeConversationMessages([user, assistant], [], run);

  assert.equal(merged[0].content, "你好");
  assert.equal(merged[1].content, "自然回复");
});

test("confirmation cards stay with their originating assistant message", () => {
  const messages = [
    message({ id: "assistant-old" }),
    message({ id: "assistant-latest" })
  ];
  const rejected = action({
    id: "rejected",
    assistant_message_id: "assistant-old",
    status: "rejected"
  });
  const proposed = action({
    id: "proposed",
    assistant_message_id: "assistant-latest",
    status: "proposed"
  });
  const unloadedPending = action({
    id: "unloaded",
    assistant_message_id: "assistant-not-loaded",
    status: "edited"
  });
  const unloadedTerminal = action({
    id: "terminal-not-loaded",
    assistant_message_id: "assistant-not-loaded",
    status: "executed"
  });

  const placement = placeConversationActions(
    messages,
    [rejected, proposed, unloadedPending, unloadedTerminal]
  );

  assert.deepEqual(placement.byMessageId["assistant-old"], [rejected]);
  assert.deepEqual(placement.byMessageId["assistant-latest"], [proposed]);
  assert.deepEqual(placement.unplacedActive, [unloadedPending]);
});

test("used context metadata maps internal labels to localized descriptors", () => {
  assert.deepEqual(
    usedContextDescriptor({ kind: "turn_summaries", label: "1 prior turns" }),
    { key: "home.contextPriorTurns", params: { count: 1 } }
  );
  assert.deepEqual(
    usedContextDescriptor({ kind: "portfolio", label: "6 positions" }),
    { key: "home.contextPositions", params: { count: 6 } }
  );
  assert.deepEqual(
    usedContextDescriptor({ kind: "investment_system", label: "rule graph v1" }),
    { key: "home.contextRuleGraph", params: { version: 1 } }
  );
});

test("portfolio constellation layout is deterministic and groups by market currency", () => {
  const first = constellationNodes([
    position({ symbol: "0700.HK", name: "Tencent", market: "HK", currency: "HKD", weight: 0.35 }),
    position({ symbol: "AAPL", name: "Apple", market: "US", currency: "USD", weight: 0.2 }),
    position({ symbol: "600000.SS", name: "浦发银行", market: "CN", currency: "CNY", weight: 0.1 })
  ]);
  const second = constellationNodes([
    position({ symbol: "0700.HK", name: "Tencent", market: "HK", currency: "HKD", weight: 0.35 }),
    position({ symbol: "AAPL", name: "Apple", market: "US", currency: "USD", weight: 0.2 }),
    position({ symbol: "600000.SS", name: "浦发银行", market: "CN", currency: "CNY", weight: 0.1 })
  ]);

  assert.deepEqual(first, second);
  assert.deepEqual(first.map((node) => node.group), ["HK/HKD", "US/USD", "CN/CNY"]);
  assert.equal(first[0].radius > first[1].radius, true);
});

function thread(overrides: Partial<MemoThreadSummary> = {}): MemoThreadSummary {
  return {
    id: "thread",
    title: "Thread",
    summary: "",
    status: "active",
    linked_symbols: [],
    tags: [],
    archived_at: null,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    last_message_at: "2026-01-01T00:00:00Z",
    ...overrides
  };
}

function position(overrides: Partial<PortfolioPosition> = {}): PortfolioPosition {
  return {
    symbol: "AAPL",
    name: "Apple",
    asset_type: "stock",
    quantity: 1,
    average_cost: 100,
    currency: "USD",
    account: null,
    market: "US",
    sector: null,
    notes: null,
    last_price: 120,
    market_value: 120,
    unrealized_pnl: 20,
    weight: 0.2,
    price_updated_at: null,
    price_stale: false,
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides
  };
}

function message(overrides: Partial<MemoThreadMessage> = {}): MemoThreadMessage {
  return {
    id: "message-1",
    thread_id: "thread-1",
    role: "assistant",
    content: "content",
    status: "completed",
    request_id: "request-1",
    duration_ms: null,
    artifacts: [],
    sources: [],
    used_context: [],
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides
  };
}

function liveRun(
  overrides: Partial<ConversationRun & {
    streamContent: string;
    providerStage?: string;
    toolSubject?: string;
  }> = {}
) {
  return {
    id: "run-1",
    client_request_id: "request-1",
    thread_id: "thread-1",
    user_message_id: "user-1",
    status: "running" as const,
    phase: "generating" as const,
    started_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:01Z",
    streamContent: "",
    ...overrides
  };
}

function action(overrides: Partial<ConversationAction> = {}): ConversationAction {
  return {
    id: "action-1",
    run_id: "run-1",
    assistant_message_id: "assistant-1",
    thread_id: "thread-1",
    action_type: "company_view_patch",
    title: "Update view",
    rationale: "New evidence",
    payload: {},
    result: null,
    target_version: 0,
    status: "proposed",
    error: null,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    executed_at: null,
    ...overrides
  };
}
