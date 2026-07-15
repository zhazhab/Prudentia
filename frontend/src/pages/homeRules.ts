import type {
  ConversationAction,
  ConversationRun,
  ConversationRunPhase,
  MemoThreadMessage,
  MemoThreadSummary,
  PortfolioPosition,
  TaskRouteReason
} from "../types/domain";
import type { TranslationKey } from "../i18n";

export interface LiveConversationRun extends ConversationRun {
  streamContent: string;
  messageId?: string;
  providerStage?: string;
  sourceCount?: number;
}

export function mergeStoredActiveRun(
  incoming: LiveConversationRun,
  existing?: LiveConversationRun
): LiveConversationRun {
  if (!existing || existing.id !== incoming.id) return incoming;
  if (Date.parse(existing.updated_at) > Date.parse(incoming.updated_at)) {
    return { ...incoming, ...existing };
  }
  return {
    ...incoming,
    streamContent: existing.streamContent,
    messageId: incoming.messageId ?? existing.messageId,
    providerStage: incoming.providerStage ?? existing.providerStage,
    sourceCount: incoming.sourceCount ?? existing.sourceCount
  };
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

export interface UsedContextDescriptor {
  key: TranslationKey;
  params: Record<string, string | number>;
}

export interface RunActivityDescriptor {
  key: TranslationKey;
  params: Record<string, string | number>;
}

export function taskComplexityKey(complexity?: string | null): TranslationKey | null {
  if (complexity === "simple") return "home.taskSimple";
  if (complexity === "standard") return "home.taskStandard";
  if (complexity === "deep") return "home.taskDeep";
  return null;
}

const routeReasonKeys: Record<TaskRouteReason, TranslationKey> = {
  social_turn: "home.routeReasonSocial",
  short_question: "home.routeReasonShort",
  subject_clarification: "home.routeReasonSubjectClarification",
  attachment_analysis: "home.routeReasonAttachment",
  investment_system: "home.routeReasonInvestmentSystem",
  multi_part_request: "home.routeReasonMultiPart",
  long_request: "home.routeReasonLong",
  explicit_deep_analysis: "home.routeReasonDeepAnalysis",
  company_research: "home.routeReasonCompanyResearch",
  standard_conversation: "home.routeReasonStandard"
};

export function parseTaskRouteReason(value: unknown): TaskRouteReason | undefined {
  return typeof value === "string" && value in routeReasonKeys
    ? value as TaskRouteReason
    : undefined;
}

export function taskRouteReasonKey(reason?: string | null): TranslationKey | null {
  const parsed = parseTaskRouteReason(reason);
  return parsed ? routeReasonKeys[parsed] : null;
}

const constellationColors = ["#2f6f73", "#8b5d33", "#7a4a63", "#4f6f37", "#53617a", "#94624f"];

export function chatHomeDefaultThreadId(threads: MemoThreadSummary[]) {
  const activeThreads = threadRailItems(threads, 50);
  if (!activeThreads.length) {
    return null;
  }

  return activeThreads[0].id;
}

export function threadRailItems<T extends MemoThreadSummary>(threads: T[], limit = 12): T[] {
  return threads.filter((thread) => !thread.archived_at && thread.status !== "deleted").slice(0, limit);
}

export function memoChatElapsedSeconds(startedAtMs: number, nowMs: number) {
  return Math.max(0, Math.floor((nowMs - startedAtMs) / 1_000));
}

export function shouldScrollConversationToBottom({
  threadId,
  pinnedThreadId,
  messageCount,
  distanceFromBottom
}: {
  threadId: string | null;
  pinnedThreadId: string | null;
  messageCount: number;
  distanceFromBottom: number;
}) {
  if (!threadId || messageCount === 0) return false;
  return threadId !== pinnedThreadId || distanceFromBottom < 140;
}

export function runActivityDescriptor(run: {
  phase: ConversationRunPhase;
  providerStage?: string;
  sourceCount?: number;
}): RunActivityDescriptor {
  if (
    run.phase === "generating" &&
    run.providerStage === "provider_reading_context" &&
    (run.sourceCount ?? 0) > 0
  ) {
    return {
      key: "home.activityReadingSources",
      params: { count: run.sourceCount ?? 0 }
    };
  }

  const researchActivityKeys: Record<string, TranslationKey> = {
    research_checking_cache: "home.activityCheckingResearchCache",
    research_cache_hit: "home.activityResearchCacheHit",
    research_fetching_public_sources: "home.activityFetchingPublicSources",
    research_fetching_financial_history: "home.activityFetchingFinancialHistory",
    research_verifying_sources: "home.activityVerifyingSources",
    research_searching_official: "home.activitySearchingOfficial",
    research_searching_independent: "home.activitySearchingIndependent",
    research_searching_community: "home.activitySearchingCommunity"
  };
  const providerActivityKeys: Record<string, TranslationKey> = {
    provider_preparing: "home.activityPreparingContext",
    process_starting: "home.activityStartingProvider",
    provider_ready: "home.activityProviderReady",
    provider_reading_context: "home.activityReadingContext",
    provider_analyzing_evidence: "home.activityAnalyzingEvidence",
    provider_using_tool: "home.activityAdditionalStep",
    provider_writing_response: "home.activityWritingResponse",
    provider_completed: "home.activityResponseReady",
    provider_failed: "home.runFailed",
    request_started: "home.activityStartingProvider",
    generating: "home.activityAnalyzingEvidence",
    provider_fallback: "home.activityProviderFallback"
  };
  const activityKey = run.providerStage
    ? run.phase === "researching"
      ? researchActivityKeys[run.providerStage]
      : run.phase === "generating"
        ? providerActivityKeys[run.providerStage]
        : undefined
    : undefined;
  if (activityKey) {
    return { key: activityKey, params: {} };
  }

  const phaseKeys: Record<ConversationRunPhase, TranslationKey> = {
    queued: "home.phaseQueued",
    resolving_subject: "home.phaseResolvingSubject",
    loading_context: "home.phaseLoadingContext",
    researching: "home.phaseResearching",
    generating: "home.phaseGenerating",
    extracting_actions: "home.phaseExtractingActions",
    persisting: "home.phasePersisting",
    completed: "home.activityResponseReady",
    failed: "home.runFailed",
    canceled: "home.runCanceled",
    interrupted: "home.runInterrupted"
  };
  return { key: phaseKeys[run.phase] ?? "home.backendRunning", params: {} };
}

export function shouldSubmitComposerMessage(event: {
  key: string;
  shiftKey: boolean;
  isComposing: boolean;
  keyCode: number;
}) {
  return (
    event.key === "Enter" &&
    !event.shiftKey &&
    !event.isComposing &&
    event.keyCode !== 229
  );
}

export function usedContextDescriptor(item: unknown): UsedContextDescriptor {
  if (!item || typeof item !== "object") {
    return {
      key: "home.contextRaw",
      params: { value: String(item ?? "") }
    };
  }

  const value = item as Record<string, unknown>;
  const kind = typeof value.kind === "string" ? value.kind : "";
  const label = String(value.label ?? kind);
  const count = firstInteger(label);

  switch (kind) {
    case "thread_summary":
      return { key: "home.contextThreadSummary", params: { value: label } };
    case "turn_summaries":
      return { key: "home.contextPriorTurns", params: { count } };
    case "portfolio":
      return { key: "home.contextPositions", params: { count } };
    case "investment_system":
      return { key: "home.contextRuleGraph", params: { version: count } };
    case "company":
      return { key: "home.contextCompanyView", params: { value: label } };
    case "attachment":
      return { key: "home.contextAttachment", params: { value: label } };
    case "source":
      return { key: "home.contextResearchSource", params: { value: label } };
    default:
      return { key: "home.contextRaw", params: { value: label } };
  }
}

function firstInteger(value: string) {
  const match = value.match(/\d+/);
  return match ? Number(match[0]) : 0;
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

export function placeConversationActions(
  messages: MemoThreadMessage[],
  actions: ConversationAction[]
) {
  const visibleMessageIds = new Set(messages.map((message) => message.id));
  const byMessageId: Record<string, ConversationAction[]> = {};
  const unplacedActive: ConversationAction[] = [];

  actions.forEach((action) => {
    const messageId = action.assistant_message_id;
    if (messageId && visibleMessageIds.has(messageId)) {
      byMessageId[messageId] = [...(byMessageId[messageId] ?? []), action];
    } else if (!["executed", "rejected"].includes(action.status)) {
      unplacedActive.push(action);
    }
  });

  return { byMessageId, unplacedActive };
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
