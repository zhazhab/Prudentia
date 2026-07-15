import type {
  AiSettings,
  ConversationAction,
  ConversationAttachment,
  ConversationRun,
  ConversationThreadDetail,
  ConversationThreadSummary,
  Memo,
  MemoExtraction,
  MemoThreadDetail,
  MemoThreadSummary,
  PortfolioDraftCommitRequest,
  PortfolioDraftPreview,
  PortfolioImportMapping,
  PortfolioImportPreview,
  PortfolioImportResult,
  PortfolioPerformancePeriod,
  PortfolioPerformanceResponse,
  PortfolioPosition,
  PriceRefreshResult,
  StartConversationRunResponse,
  ThreadSubject,
  PortfolioSummary,
  PortfolioDraftSymbolResolveResult,
  UpdateAiSettings,
  UpdatePortfolioPosition
} from "../types/domain";

const API_BASE = import.meta.env.VITE_API_BASE_URL ?? "";

async function request<T>(
  path: string,
  init: RequestInit & { languageTag?: string } = {}
): Promise<T> {
  const { languageTag, ...requestInit } = init;
  const response = await fetch(`${API_BASE}${path}`, {
    ...requestInit,
    headers: {
      "Content-Type": "application/json",
      "Accept-Language": languageTag ?? defaultLanguageTag(),
      ...(requestInit.headers ?? {})
    }
  });

  if (!response.ok) {
    const body = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(body.error ?? "Request failed");
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return response.json() as Promise<T>;
}

function defaultLanguageTag() {
  return window.localStorage.getItem("prudentia.locale") === "zh" ? "zh-CN" : "en-US";
}

export interface FilePayload {
  file_name: string;
  content: string;
  content_encoding?: "base64";
}

export interface ImagePayload {
  file_name: string;
  content: string;
  content_encoding: "base64";
  mime_type: string;
}

export const api = {
  conversationThreads: () => request<ConversationThreadSummary[]>("/api/conversation/threads"),
  conversationThread: (id: string, beforeMessageId?: string) =>
    request<ConversationThreadDetail>(
      `/api/conversation/threads/${encodeURIComponent(id)}${
        beforeMessageId ? `?before_message_id=${encodeURIComponent(beforeMessageId)}` : ""
      }`
    ),
  startConversationRun: (payload: {
    client_request_id: string;
    thread_id?: string;
    client_thread_id?: string;
    content: string;
    attachment_ids?: string[];
    locale?: string;
  }) =>
    request<StartConversationRunResponse>("/api/conversation/runs", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  activeConversationRuns: () => request<ConversationRun[]>("/api/conversation/runs/active"),
  cancelConversationRun: (id: string) =>
    request<ConversationRun>(`/api/conversation/runs/${encodeURIComponent(id)}/cancel`, {
      method: "POST"
    }),
  retryConversationRun: (id: string) =>
    request<StartConversationRunResponse>(`/api/conversation/runs/${encodeURIComponent(id)}/retry`, {
      method: "POST"
    }),
  updateConversationSubject: (
    id: string,
    payload: { kind: string; subject_key?: string | null; label?: string | null }
  ) =>
    request<ThreadSubject>(`/api/conversation/threads/${encodeURIComponent(id)}/subject`, {
      method: "PATCH",
      body: JSON.stringify(payload)
    }),
  updateConversationAction: (id: string, payload: Record<string, unknown>) =>
    request<ConversationAction>(`/api/conversation/actions/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: JSON.stringify({ payload })
    }),
  confirmConversationAction: (id: string, expectedVersion?: number | null) =>
    request<ConversationAction>(`/api/conversation/actions/${encodeURIComponent(id)}/confirm`, {
      method: "POST",
      body: JSON.stringify({ expected_version: expectedVersion ?? null })
    }),
  rejectConversationAction: (id: string) =>
    request<ConversationAction>(`/api/conversation/actions/${encodeURIComponent(id)}/reject`, {
      method: "POST",
      body: JSON.stringify({})
    }),
  uploadConversationAttachment: (payload: {
    file_name?: string;
    mime_type?: string;
    content?: string;
    content_encoding?: "base64";
    url?: string;
  }) =>
    request<ConversationAttachment>("/api/conversation/attachments", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  archiveConversationThread: (id: string) =>
    request<MemoThreadSummary>(`/api/conversation/threads/${encodeURIComponent(id)}/archive`, {
      method: "POST"
    }),
  deleteConversationThread: (id: string) =>
    request<MemoThreadSummary>(`/api/conversation/threads/${encodeURIComponent(id)}`, {
      method: "DELETE"
    }),
  memos: () => request<Memo[]>("/api/memos/"),
  memoThreads: () => request<MemoThreadSummary[]>("/api/memo-threads/"),
  memoThread: (id: string, beforeMessageId?: string) =>
    request<MemoThreadDetail>(
      `/api/memo-threads/${encodeURIComponent(id)}${
        beforeMessageId ? `?before_message_id=${encodeURIComponent(beforeMessageId)}` : ""
      }`
    ),
  archiveMemoThread: (id: string) =>
    request<MemoThreadSummary>(`/api/memo-threads/${encodeURIComponent(id)}/archive`, {
      method: "POST"
    }),
  deleteMemoThread: (id: string) =>
    request<MemoThreadSummary>(`/api/memo-threads/${encodeURIComponent(id)}`, {
      method: "DELETE"
    }),
  createMemo: (payload: Partial<Memo> & { title: string }) =>
    request<Memo>("/api/memos/", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  extractMemo: (id: string, languageTag?: string) =>
    request<MemoExtraction>(`/api/memos/${id}/ai/extract`, { method: "POST", languageTag }),
  positions: (period?: PortfolioPerformancePeriod) =>
    request<PortfolioPosition[]>(
      period ? `/api/portfolio/positions?period=${encodeURIComponent(period)}` : "/api/portfolio/positions"
    ),
  portfolioSummary: () => request<PortfolioSummary>("/api/portfolio/summary"),
  portfolioPerformance: (period: PortfolioPerformancePeriod) =>
    request<PortfolioPerformanceResponse>(`/api/portfolio/performance?period=${encodeURIComponent(period)}`),
  refreshPortfolioPrices: () =>
    request<PriceRefreshResult>("/api/portfolio/prices/refresh", {
      method: "POST"
    }),
  previewPortfolioImport: (payload: FilePayload) =>
    request<PortfolioImportPreview>("/api/portfolio/import/preview", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  draftPortfolioImport: (payload: FilePayload & { mapping: PortfolioImportMapping }) =>
    request<PortfolioDraftPreview>("/api/portfolio/import/draft", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  commitPortfolioDraft: (payload: PortfolioDraftCommitRequest) =>
    request<PortfolioImportResult>("/api/portfolio/import/draft/commit", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  updatePosition: (symbol: string, payload: UpdatePortfolioPosition) =>
    request<PortfolioPosition>(`/api/portfolio/positions/${encodeURIComponent(symbol)}`, {
      method: "PATCH",
      body: JSON.stringify(payload)
    }),
  deletePosition: (symbol: string) =>
    request<PortfolioPosition[]>(`/api/portfolio/positions/${encodeURIComponent(symbol)}`, {
      method: "DELETE"
    }),
  resolvePortfolioDraftSymbols: (payload: PortfolioDraftCommitRequest) =>
    request<PortfolioDraftSymbolResolveResult>("/api/portfolio/symbols/resolve-draft", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  aiSettings: () => request<AiSettings>("/api/settings/ai"),
  updateAiSettings: (payload: UpdateAiSettings) =>
    request<AiSettings>("/api/settings/ai", {
      method: "PATCH",
      body: JSON.stringify(payload)
    })
};
