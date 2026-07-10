import type { UpdateAiSettings } from "../types/domain";

export type AiProviderMode = "mock" | "openai" | "cli";

export function providerMode(provider: string | null | undefined): AiProviderMode {
  const normalized = provider?.trim().toLowerCase();
  if (normalized === "openai") {
    return "openai";
  }

  if (normalized === "cli" || normalized === "codex" || normalized === "codex_cli") {
    return "cli";
  }

  return normalized === "mock" ? "mock" : "cli";
}

export function aiSettingsPayload(draft: UpdateAiSettings): UpdateAiSettings {
  const mode = providerMode(draft.provider);
  const payload: UpdateAiSettings = {
    provider: mode,
    persist_to_env: true
  };

  if (mode === "openai") {
    payload.openai_base_url = cleanOrDefault(draft.openai_base_url, "https://api.openai.com/v1");
    payload.openai_model = cleanOrDefault(draft.openai_model, "gpt-4.1-mini");

    const apiKey = cleanOptional(draft.openai_api_key);
    if (apiKey) {
      payload.openai_api_key = apiKey;
    }
  }

  if (mode === "cli") {
    payload.cli_provider = cleanOrDefault(draft.cli_provider, "codex");
    payload.cli_path = cleanOrDefault(draft.cli_path, "codex");
    payload.cli_model = cleanValue(draft.cli_model);
    payload.cli_profile = cleanValue(draft.cli_profile);
  }

  return payload;
}

function cleanOrDefault(value: string | undefined, fallback: string) {
  return cleanOptional(value) ?? fallback;
}

function cleanOptional(value: string | undefined) {
  const cleaned = cleanValue(value);
  return cleaned || undefined;
}

function cleanValue(value: string | undefined) {
  return value?.trim() ?? "";
}
