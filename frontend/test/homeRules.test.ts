import assert from "node:assert/strict";
import test from "node:test";
import {
  chatHomeDefaultThreadId,
  constellationNodes,
  mergeConversationMessages,
  memoChatElapsedSeconds,
  shouldSubmitComposerMessage,
  threadRailItems,
  usedContextDescriptor
} from "../src/pages/homeRules.ts";
import type {
  ConversationRun,
  MemoThreadMessage,
  MemoThreadSummary,
  PortfolioPosition
} from "../src/types/domain.ts";

test("chat home restores last thread when it is available", () => {
  const threads = [
    thread({ id: "recent", title: "Recent" }),
    thread({ id: "last", title: "Last" })
  ];

  assert.equal(chatHomeDefaultThreadId(threads, "last"), "last");
  assert.equal(chatHomeDefaultThreadId(threads, "missing"), "recent");
  assert.equal(chatHomeDefaultThreadId([], "last"), null);
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
