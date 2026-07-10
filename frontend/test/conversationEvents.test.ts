import assert from "node:assert/strict";
import test from "node:test";
import {
  conversationEventUrl,
  parseConversationEvent
} from "../src/api/conversationEvents.ts";

test("conversation events carry a durable cursor and run identity", () => {
  const event = parseConversationEvent(
    JSON.stringify({
      event_id: 42,
      run_id: "run-1",
      thread_id: "thread-1",
      event_type: "run.phase",
      payload: { phase: "generating", provider: "cli" },
      created_at: "2026-07-10T00:00:00Z"
    })
  );

  assert.equal(event.event_id, 42);
  assert.equal(event.payload.phase, "generating");
});

test("conversation event websocket resumes after the last event id", () => {
  assert.equal(
    conversationEventUrl("http://127.0.0.1:8080", 41, "http://127.0.0.1:5173"),
    "ws://127.0.0.1:8080/api/conversation/events/ws?after_event_id=41"
  );
  assert.equal(
    conversationEventUrl("https://example.com/api", -1, "https://example.com"),
    "wss://example.com/api/conversation/events/ws?after_event_id=0"
  );
});

test("conversation events reject missing durable ids", () => {
  assert.throws(
    () => parseConversationEvent('{"run_id":"run-1","thread_id":"thread-1","event_type":"run.phase","payload":{}}'),
    /event_id/
  );
});
