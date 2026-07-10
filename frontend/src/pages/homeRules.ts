import type {
  ConversationRun,
  MemoThreadMessage,
  MemoThreadSummary,
  PortfolioPosition
} from "../types/domain";

export interface LiveConversationRun extends ConversationRun {
  streamContent: string;
  messageId?: string;
  providerStage?: string;
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

const constellationColors = ["#2f6f73", "#8b5d33", "#7a4a63", "#4f6f37", "#53617a", "#94624f"];

export function chatHomeDefaultThreadId(
  threads: MemoThreadSummary[],
  lastThreadId?: string | null
) {
  const activeThreads = threadRailItems(threads, 50);
  if (!activeThreads.length) {
    return null;
  }

  if (lastThreadId && activeThreads.some((thread) => thread.id === lastThreadId)) {
    return lastThreadId;
  }

  return activeThreads[0].id;
}

export function threadRailItems<T extends MemoThreadSummary>(threads: T[], limit = 12): T[] {
  return threads.filter((thread) => !thread.archived_at && thread.status !== "deleted").slice(0, limit);
}

export function memoChatElapsedSeconds(startedAtMs: number, nowMs: number) {
  return Math.max(0, Math.floor((nowMs - startedAtMs) / 1_000));
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
