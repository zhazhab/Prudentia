import assert from "node:assert/strict";
import test from "node:test";
import { AiWebSocketClient, AiWebSocketSession, parseAiWsMessage, websocketUrl } from "../src/api/aiWs.ts";

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

test("parses memo chat websocket delta messages", () => {
  assert.deepEqual(
    parseAiWsMessage(
      JSON.stringify({
        type: "delta",
        request_id: "req-1",
        thread_id: "thread-1",
        content: "第一段"
      })
    ),
    {
      type: "delta",
      request_id: "req-1",
      thread_id: "thread-1",
      content: "第一段"
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
  assert.equal(
    websocketUrl("", "/api/ai/ws", "http://127.0.0.1:5173"),
    "ws://127.0.0.1:5173/api/ai/ws"
  );
});

test("ignores connection errors from sockets closed by the client", async () => {
  const originalWebSocket = globalThis.WebSocket;
  const sockets: MockWebSocket[] = [];

  class MockWebSocket {
    static OPEN = 1;
    readyState = 0;
    readonly url: string;
    onopen: (() => void) | null = null;
    onerror: (() => void) | null = null;
    onmessage: ((event: { data: string }) => void) | null = null;
    onclose: (() => void) | null = null;

    constructor(url: string) {
      this.url = url;
      sockets.push(this);
    }

    send() {}

    close() {
      this.readyState = 3;
      this.onclose?.();
    }

    fail() {
      this.onerror?.();
    }
  }

  globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;
  try {
    const client = new AiWebSocketClient("ws://127.0.0.1:8080/api/ai/ws");
    let rejected: Error | null = null;
    client.connect().catch((error: Error) => {
      rejected = error;
    });

    client.close();
    sockets[0].fail();
    await new Promise((resolve) => setTimeout(resolve, 0));

    assert.equal(rejected, null);
  } finally {
    globalThis.WebSocket = originalWebSocket;
  }
});

test("ai websocket sessions can connect before first task", async () => {
  let created = 0;
  let subscribed = 0;
  let connected = 0;
  let closed = 0;
  const sent: unknown[] = [];

  const session = new AiWebSocketSession(() => {
    created += 1;
    return {
      connect() {
        connected += 1;
        return Promise.resolve();
      },
      onMessage() {
        subscribed += 1;
        return () => {
          subscribed -= 1;
        };
      },
      send(message: unknown) {
        sent.push(message);
        return Promise.resolve();
      },
      close() {
        closed += 1;
      }
    };
  });

  assert.equal(created, 0);
  assert.equal(session.hasClient(), false);

  await session.connect();

  assert.equal(created, 1);
  assert.equal(connected, 1);
  assert.equal(subscribed, 1);
  assert.equal(session.hasClient(), true);

  session.send({ type: "cancel", request_id: "req-1" });

  assert.equal(created, 1);
  assert.equal(connected, 1);
  assert.equal(subscribed, 1);
  assert.equal(session.hasClient(), true);
  assert.deepEqual(sent, [{ type: "cancel", request_id: "req-1" }]);

  session.close();

  assert.equal(closed, 1);
  assert.equal(subscribed, 0);
  assert.equal(session.hasClient(), false);
});

test("ai websocket sessions fan out server messages to subscribers", () => {
  let serverMessageHandler: ((message: { type: string; request_id: string }) => void) | null = null;
  const seen: unknown[] = [];
  const session = new AiWebSocketSession(() => ({
    connect() {
      return Promise.resolve();
    },
    onMessage(handler) {
      serverMessageHandler = handler;
      return () => {
        serverMessageHandler = null;
      };
    },
    send() {
      return Promise.resolve();
    },
    close() {}
  }));

  const unsubscribe = session.onMessage((message) => {
    seen.push(message);
  });

  session.getClient();
  serverMessageHandler?.({ type: "accepted", request_id: "req-1" });

  assert.deepEqual(seen, [{ type: "accepted", request_id: "req-1" }]);

  unsubscribe();
  serverMessageHandler?.({ type: "canceled", request_id: "req-1" });

  assert.deepEqual(seen, [{ type: "accepted", request_id: "req-1" }]);
});
