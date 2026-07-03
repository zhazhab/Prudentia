import assert from "node:assert/strict";
import test from "node:test";
import { parseAiWsMessage, websocketUrl } from "../src/api/aiWs.ts";

test("parses ai websocket messages by request id", () => {
  assert.deepEqual(
    parseAiWsMessage(
      JSON.stringify({
        type: "progress",
        request_id: "req-1",
        stage: "recognizing_image"
      })
    ),
    {
      type: "progress",
      request_id: "req-1",
      stage: "recognizing_image"
    }
  );
});

test("rejects websocket messages without a request id", () => {
  assert.throws(
    () => parseAiWsMessage(JSON.stringify({ type: "completed" })),
    /request_id/
  );
});

test("builds websocket urls from api base urls", () => {
  assert.equal(websocketUrl("http://127.0.0.1:8080", "/api/ai/ws"), "ws://127.0.0.1:8080/api/ai/ws");
  assert.equal(websocketUrl("https://example.com/api", "/ai/ws"), "wss://example.com/api/ai/ws");
  assert.equal(websocketUrl("", "/api/ai/ws"), "/api/ai/ws");
});
