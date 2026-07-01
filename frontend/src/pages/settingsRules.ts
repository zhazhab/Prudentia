import type { TranslationKey } from "../i18n";
import type { UpdateAiSettings } from "../types/domain";

export type AiProviderMode = "mock" | "openai" | "cli";

interface ProviderSettingsView {
  provider: AiProviderMode;
  showOpenAi: boolean;
  showCli: boolean;
  helpTitleKey: TranslationKey;
  helpBodyKey: TranslationKey;
}

export function providerSettingsView(provider: string | undefined): ProviderSettingsView {
  const activeProvider = normalizeProvider(provider);

  return {
    provider: activeProvider,
    showOpenAi: activeProvider === "openai",
    showCli: activeProvider === "cli",
    helpTitleKey: providerTitleKey(activeProvider),
    helpBodyKey: providerBodyKey(activeProvider)
  };
}

export function buildAiSettingsPayload(draft: UpdateAiSettings): UpdateAiSettings {
  const provider = normalizeProvider(draft.provider);
  const payload: UpdateAiSettings = {
    provider,
    persist_to_env: true
  };

  if (provider === "openai") {
    payload.openai_base_url = draft.openai_base_url ?? "";
    payload.openai_model = draft.openai_model ?? "";

    const openaiApiKey = draft.openai_api_key?.trim();
    if (openaiApiKey) {
      payload.openai_api_key = openaiApiKey;
    }
  }

  if (provider === "cli") {
    payload.cli_provider = draft.cli_provider ?? "codex";
    payload.cli_path = draft.cli_path ?? "codex";
    payload.cli_model = draft.cli_model ?? "";
    payload.cli_profile = draft.cli_profile ?? "";
  }

  return payload;
}

export function shouldExpandCliAdvanced(
  cliModel: string | null | undefined,
  cliProfile: string | null | undefined
) {
  return Boolean(cliModel?.trim() || cliProfile?.trim());
}

function normalizeProvider(provider: string | undefined): AiProviderMode {
  if (provider === "openai" || provider === "cli") {
    return provider;
  }

  return "mock";
}

function providerTitleKey(provider: AiProviderMode): TranslationKey {
  if (provider === "openai") {
    return "settings.providerOpenai";
  }
  if (provider === "cli") {
    return "settings.providerCli";
  }

  return "settings.providerMock";
}

function providerBodyKey(provider: AiProviderMode): TranslationKey {
  if (provider === "openai") {
    return "settings.openaiNote";
  }
  if (provider === "cli") {
    return "settings.cliNote";
  }

  return "settings.mockNote";
}
