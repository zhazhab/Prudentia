import type { ImagePayload } from "./client";

const DEFAULT_AI_WS_PATH = "/api/ai/ws";
const API_BASE = import.meta.env?.VITE_API_BASE_URL ?? "";

export type AiWsClientMessage =
  | {
      type: "portfolio_image_import.start";
      request_id: string;
      payload: ImagePayload;
    }
  | {
      type: "memo_chat.start";
      request_id: string;
      thread_id?: string;
      client_thread_id?: string;
      locale?: string;
      message: {
        content: string;
      };
      context_hints?: {
        last_thread_id?: string | null;
      };
    }
  | {
      type: "cancel";
      request_id: string;
    };

export type AiWsServerMessage =
  | { type: "accepted"; request_id: string; thread_id?: string | null }
  | { type: "progress"; request_id: string; stage: string }
  | { type: "delta"; request_id: string; thread_id: string; content: string }
  | { type: "artifact"; request_id: string; thread_id: string; artifact_type: string; data: unknown }
  | {
      type: "completed";
      request_id: string;
      artifact_type: string;
      data: unknown;
      thread_id?: string | null;
      message_id?: string | null;
      duration_ms?: number | null;
    }
  | {
      type: "failed";
      request_id: string;
      code: string;
      error: string;
      thread_id?: string | null;
      duration_ms?: number | null;
    }
  | { type: "canceled"; request_id: string; thread_id?: string | null; duration_ms?: number | null };

export type AiWsMessageHandler = (message: AiWsServerMessage) => void;
export type AiWebSocketConnection = Pick<AiWebSocketClient, "connect" | "onMessage" | "send" | "close">;

export class AiWebSocketClient {
  private socket: WebSocket | null = null;
  private opening: Promise<void> | null = null;
  private openingResolve: (() => void) | null = null;
  private attemptId = 0;
  private readonly handlers = new Set<AiWsMessageHandler>();
  private readonly url: string;

  constructor(url = websocketUrl(API_BASE, DEFAULT_AI_WS_PATH)) {
    this.url = url;
  }

  connect() {
    if (this.socket?.readyState === WebSocket.OPEN) {
      return Promise.resolve();
    }
    if (this.opening) {
      return this.opening;
    }

    const attemptId = ++this.attemptId;
    this.opening = new Promise<void>((resolve, reject) => {
      const socket = new WebSocket(this.url);
      this.socket = socket;
      this.openingResolve = resolve;

      const isCurrentSocket = () => this.attemptId === attemptId && this.socket === socket;
      const clearOpening = () => {
        if (isCurrentSocket()) {
          this.opening = null;
          this.openingResolve = null;
        }
      };

      socket.onopen = () => {
        clearOpening();
        resolve();
      };
      socket.onerror = () => {
        if (!isCurrentSocket()) {
          return;
        }
        clearOpening();
        this.socket = null;
        reject(new Error("AI WebSocket connection failed"));
      };
      socket.onmessage = (event) => {
        const message = parseAiWsMessage(String(event.data));
        this.handlers.forEach((handler) => handler(message));
      };
      socket.onclose = () => {
        if (!isCurrentSocket()) {
          return;
        }
        const wasOpening = this.opening !== null;
        clearOpening();
        this.socket = null;
        if (wasOpening) {
          reject(new Error("AI WebSocket connection closed before it opened"));
        }
      };
    });

    return this.opening;
  }

  onMessage(handler: AiWsMessageHandler) {
    this.handlers.add(handler);
    return () => {
      this.handlers.delete(handler);
    };
  }

  async send(message: AiWsClientMessage) {
    await this.connect();
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      throw new Error("AI WebSocket is not connected");
    }
    this.socket.send(JSON.stringify(message));
  }

  close() {
    this.attemptId += 1;
    this.openingResolve?.();
    this.opening = null;
    this.openingResolve = null;
    this.socket?.close();
    this.socket = null;
  }
}

export class AiWebSocketSession {
  private client: AiWebSocketConnection | null = null;
  private unsubscribe: (() => void) | null = null;
  private readonly handlers = new Set<AiWsMessageHandler>();
  private readonly createClient: () => AiWebSocketConnection;

  constructor(createClient: () => AiWebSocketConnection = () => new AiWebSocketClient()) {
    this.createClient = createClient;
  }

  hasClient() {
    return this.client !== null;
  }

  getClient() {
    if (!this.client) {
      this.client = this.createClient();
      this.unsubscribe = this.client.onMessage((message) => {
        this.handlers.forEach((handler) => handler(message));
      });
    }
    return this.client;
  }

  onMessage(handler: AiWsMessageHandler) {
    this.handlers.add(handler);
    return () => {
      this.handlers.delete(handler);
    };
  }

  connect() {
    return this.getClient().connect();
  }

  send(message: AiWsClientMessage) {
    return this.getClient().send(message);
  }

  close() {
    this.unsubscribe?.();
    this.client?.close();
    this.unsubscribe = null;
    this.client = null;
  }
}

export function parseAiWsMessage(raw: string): AiWsServerMessage {
  const parsed = JSON.parse(raw) as Partial<AiWsServerMessage>;
  if (!parsed || typeof parsed !== "object") {
    throw new Error("AI WebSocket message must be an object");
  }
  if (!("type" in parsed) || typeof parsed.type !== "string") {
    throw new Error("AI WebSocket message is missing type");
  }
  if (!("request_id" in parsed) || typeof parsed.request_id !== "string") {
    throw new Error("AI WebSocket message is missing request_id");
  }
  return parsed as AiWsServerMessage;
}

export function websocketUrl(apiBase: string, path: string, pageOrigin = currentPageOrigin()) {
  const base = apiBase || pageOrigin;
  if (!base) {
    return path;
  }

  const normalizedPath = path.replace(/^\/+/, "");
  const url = new URL(normalizedPath, base.endsWith("/") ? base : `${base}/`);
  if (url.protocol === "https:") {
    url.protocol = "wss:";
  } else if (url.protocol === "http:") {
    url.protocol = "ws:";
  }
  return url.toString();
}

function currentPageOrigin() {
  return globalThis.location?.origin ?? "";
}
