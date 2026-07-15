import type { RunEvent } from "../types/domain";

const API_BASE = import.meta.env?.VITE_API_BASE_URL ?? "";
const cursorStorageKey = "prudentia.conversationEventCursor";

export type ConversationEventHandler = (event: RunEvent) => void;
export type ConversationConnectionHandler = (error: string | null) => void;

export class ConversationEventClient {
  private socket: WebSocket | null = null;
  private reconnectTimer: number | null = null;
  private closed = false;
  private cursor = initialCursor();
  private readonly handlers = new Set<ConversationEventHandler>();
  private readonly connectionHandlers = new Set<ConversationConnectionHandler>();
  private readonly baseUrl: string;

  constructor(baseUrl = API_BASE) {
    this.baseUrl = baseUrl;
  }

  connect() {
    if (
      this.socket &&
      (this.socket.readyState === WebSocket.OPEN || this.socket.readyState === WebSocket.CONNECTING)
    ) {
      return;
    }
    this.closed = false;
    const socket = new WebSocket(conversationEventUrl(this.baseUrl, this.cursor));
    this.socket = socket;
    socket.onopen = () => this.connectionHandlers.forEach((handler) => handler(null));
    socket.onmessage = (message) => {
      const event = parseConversationEvent(String(message.data));
      this.cursor = event.event_id;
      persistCursor(this.cursor);
      this.handlers.forEach((handler) => handler(event));
    };
    socket.onerror = () => {
      this.connectionHandlers.forEach((handler) => handler("Conversation event connection failed"));
    };
    socket.onclose = () => {
      if (this.socket === socket) {
        this.socket = null;
      }
      if (!this.closed) {
        this.reconnectTimer = window.setTimeout(() => this.connect(), 1_000);
      }
    };
  }

  onEvent(handler: ConversationEventHandler) {
    this.handlers.add(handler);
    return () => {
      this.handlers.delete(handler);
    };
  }

  onConnection(handler: ConversationConnectionHandler) {
    this.connectionHandlers.add(handler);
    return () => {
      this.connectionHandlers.delete(handler);
    };
  }

  close() {
    this.closed = true;
    if (this.reconnectTimer !== null) {
      window.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.socket?.close();
    this.socket = null;
  }
}

export function parseConversationEvent(raw: string): RunEvent {
  const event = JSON.parse(raw) as RunEvent;
  if (!event || typeof event !== "object" || typeof event.event_id !== "number") {
    throw new Error("Conversation event is missing event_id");
  }
  if (typeof event.run_id !== "string" || typeof event.thread_id !== "string") {
    throw new Error("Conversation event is missing run or thread id");
  }
  if (typeof event.event_type !== "string" || !event.payload || typeof event.payload !== "object") {
    throw new Error("Conversation event is invalid");
  }
  return event;
}

export function conversationEventUrl(
  apiBase: string,
  afterEventId: number,
  pageOrigin = currentPageOrigin()
) {
  const base = apiBase || pageOrigin;
  const path = `/api/conversation/events/ws?after_event_id=${Math.max(0, afterEventId)}`;
  if (!base) {
    return path;
  }
  const url = new URL(path, base.endsWith("/") ? base : `${base}/`);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return url.toString();
}

function initialCursor() {
  if (typeof window === "undefined") {
    return 0;
  }
  const value = Number(window.sessionStorage.getItem(cursorStorageKey));
  return Number.isFinite(value) && value > 0 ? value : 0;
}

function persistCursor(cursor: number) {
  if (typeof window !== "undefined") {
    window.sessionStorage.setItem(cursorStorageKey, String(cursor));
  }
}

function currentPageOrigin() {
  return typeof window === "undefined" ? "" : window.location.origin;
}
