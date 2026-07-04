import type {
  AiSettings,
  Memo,
  MemoExtraction,
  PortfolioDraftCommitRequest,
  PortfolioDraftPreview,
  PortfolioImportMapping,
  PortfolioImportPreview,
  PortfolioImportResult,
  PortfolioPerformancePeriod,
  PortfolioPerformanceResponse,
  PortfolioPosition,
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
  memos: () => request<Memo[]>("/api/memos/"),
  createMemo: (payload: Partial<Memo> & { title: string }) =>
    request<Memo>("/api/memos/", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  extractMemo: (id: string, languageTag?: string) =>
    request<MemoExtraction>(`/api/memos/${id}/ai/extract`, { method: "POST", languageTag }),
  positions: () => request<PortfolioPosition[]>("/api/portfolio/positions"),
  portfolioSummary: () => request<PortfolioSummary>("/api/portfolio/summary"),
  portfolioPerformance: (period: PortfolioPerformancePeriod) =>
    request<PortfolioPerformanceResponse>(`/api/portfolio/performance?period=${encodeURIComponent(period)}`),
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
