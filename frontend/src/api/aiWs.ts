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
      type: "cancel";
      request_id: string;
    };

export type AiWsServerMessage =
  | { type: "accepted"; request_id: string }
  | { type: "progress"; request_id: string; stage: string }
  | { type: "completed"; request_id: string; artifact_type: string; data: unknown }
  | { type: "failed"; request_id: string; code: string; error: string }
  | { type: "canceled"; request_id: string };

export type AiWsMessageHandler = (message: AiWsServerMessage) => void;

export class AiWebSocketClient {
  private socket: WebSocket | null = null;
  private opening: Promise<void> | null = null;
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

    this.opening = new Promise<void>((resolve, reject) => {
      const socket = new WebSocket(this.url);
      this.socket = socket;

      socket.onopen = () => {
        this.opening = null;
        resolve();
      };
      socket.onerror = () => {
        this.opening = null;
        reject(new Error("AI WebSocket connection failed"));
      };
      socket.onmessage = (event) => {
        const message = parseAiWsMessage(String(event.data));
        this.handlers.forEach((handler) => handler(message));
      };
      socket.onclose = () => {
        this.opening = null;
        if (this.socket === socket) {
          this.socket = null;
        }
      };
    });

    return this.opening;
  }

  onMessage(handler: AiWsMessageHandler) {
    this.handlers.add(handler);
    return () => this.handlers.delete(handler);
  }

  async send(message: AiWsClientMessage) {
    await this.connect();
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      throw new Error("AI WebSocket is not connected");
    }
    this.socket.send(JSON.stringify(message));
  }

  close() {
    this.socket?.close();
    this.socket = null;
    this.opening = null;
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

export function websocketUrl(apiBase: string, path: string) {
  if (!apiBase) {
    return path;
  }

  const normalizedPath = path.replace(/^\/+/, "");
  const url = new URL(normalizedPath, apiBase.endsWith("/") ? apiBase : `${apiBase}/`);
  if (url.protocol === "https:") {
    url.protocol = "wss:";
  } else if (url.protocol === "http:") {
    url.protocol = "ws:";
  }
  return url.toString();
}
