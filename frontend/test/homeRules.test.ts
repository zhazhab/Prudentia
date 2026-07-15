import assert from "node:assert/strict";
import test from "node:test";
import {
  chatHomeDefaultThreadId,
  constellationNodes,
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
  PortfolioPosition
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
  overrides: Partial<ConversationRun & { streamContent: string; providerStage?: string }> = {}
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
