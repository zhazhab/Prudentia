import type {
  InvestmentSystem,
  InvestmentSystemRefinement,
  InvestorProfile,
  AdoptResearchCandidatesRequest,
  AiSettings,
  DistillResearchRequest,
  Memo,
  MemoExtraction,
  PortfolioImportMapping,
  PortfolioImportPreview,
  PortfolioImportResult,
  PortfolioPosition,
  PortfolioSummary,
  PriceRefreshResult,
  ResearchRecord,
  StockSnapshotRequest,
  UpdateAiSettings
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

function queryString(params: Record<string, string | undefined>) {
  const search = new URLSearchParams();
  Object.entries(params).forEach(([key, value]) => {
    if (value?.trim()) {
      search.set(key, value.trim());
    }
  });
  const value = search.toString();
  return value ? `?${value}` : "";
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
  investmentSystem: (languageTag?: string) =>
    request<InvestmentSystem>("/api/investment-system/", { languageTag }),
  updateInvestmentSystem: (payload: Partial<InvestmentSystem>) =>
    request<InvestmentSystem>("/api/investment-system/", {
      method: "PATCH",
      body: JSON.stringify(payload)
    }),
  refineInvestmentSystem: (languageTag?: string) =>
    request<InvestmentSystemRefinement>("/api/investment-system/ai/refine", {
      method: "POST",
      languageTag
    }),
  researchRecords: (params: { kind?: string; symbol?: string; q?: string } = {}) =>
    request<ResearchRecord[]>(`/api/research/records${queryString(params)}`),
  researchRecord: (id: string) => request<ResearchRecord>(`/api/research/records/${id}`),
  distillResearch: (payload: DistillResearchRequest, languageTag?: string) =>
    request<ResearchRecord>("/api/research/distill", {
      method: "POST",
      body: JSON.stringify(payload),
      languageTag
    }),
  stockSnapshot: (payload: StockSnapshotRequest, languageTag?: string) =>
    request<ResearchRecord>("/api/research/stock-snapshot", {
      method: "POST",
      body: JSON.stringify(payload),
      languageTag
    }),
  portfolioReview: (languageTag?: string) =>
    request<ResearchRecord>("/api/research/portfolio-review", { method: "POST", languageTag }),
  adoptResearchCandidates: (
    id: string,
    payload: AdoptResearchCandidatesRequest,
    languageTag?: string
  ) =>
    request<InvestmentSystem>(`/api/research/records/${id}/adopt`, {
      method: "POST",
      body: JSON.stringify(payload),
      languageTag
    }),
  positions: () => request<PortfolioPosition[]>("/api/portfolio/positions"),
  portfolioSummary: () => request<PortfolioSummary>("/api/portfolio/summary"),
  previewPortfolioImport: (payload: FilePayload) =>
    request<PortfolioImportPreview>("/api/portfolio/import/preview", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  commitPortfolioImport: (payload: FilePayload & { mapping: PortfolioImportMapping }) =>
    request<PortfolioImportResult>("/api/portfolio/import/commit", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  refreshPrices: () =>
    request<PriceRefreshResult>("/api/portfolio/prices/refresh", { method: "POST" }),
  profile: (languageTag?: string) => request<InvestorProfile>("/api/profile", { languageTag }),
  aiSettings: () => request<AiSettings>("/api/settings/ai"),
  updateAiSettings: (payload: UpdateAiSettings) =>
    request<AiSettings>("/api/settings/ai", {
      method: "PATCH",
      body: JSON.stringify(payload)
    })
};
