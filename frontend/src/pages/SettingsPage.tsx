import { FormEvent, useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Save } from "lucide-react";
import { api } from "../api/client";
import { EmptyState } from "../components/EmptyState";
import { useI18n, type TranslationKey } from "../i18n";
import type { UpdateAiSettings } from "../types/domain";
import { aiSettingsPayload, providerMode, type AiProviderMode } from "./settingsRules";

export function SettingsPage() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const settings = useQuery({ queryKey: ["ai-settings"], queryFn: api.aiSettings });
  const [saved, setSaved] = useState(false);
  const [draft, setDraft] = useState<UpdateAiSettings>({
    provider: "mock",
    openai_base_url: "https://api.openai.com/v1",
    openai_model: "gpt-4.1-mini",
    cli_provider: "codex",
    cli_path: "codex",
    cli_model: "",
    cli_profile: "",
    openai_api_key: "",
    persist_to_env: true
  });
  const mode = providerMode(draft.provider);
  const helpCopy = providerHelpCopy(mode);

  useEffect(() => {
    if (!settings.data) {
      return;
    }
    setDraft({
      provider: settings.data.provider,
      openai_base_url: settings.data.openai_base_url,
      openai_model: settings.data.openai_model,
      cli_provider: settings.data.cli_provider,
      cli_path: settings.data.cli_path,
      cli_model: settings.data.cli_model ?? "",
      cli_profile: settings.data.cli_profile ?? "",
      openai_api_key: "",
      persist_to_env: true
    });
  }, [settings.data]);

  const save = useMutation({
    mutationFn: api.updateAiSettings,
    onSuccess: () => {
      setSaved(true);
      queryClient.invalidateQueries({ queryKey: ["ai-settings"] });
    }
  });

  function submit(event: FormEvent) {
    event.preventDefault();
    setSaved(false);
    save.mutate(aiSettingsPayload(draft));
  }

  return (
    <div className="page-stack">
      <header className="page-header">
        <div>
          <span className="eyebrow">{t("settings.eyebrow")}</span>
          <h2>{t("settings.title")}</h2>
        </div>
      </header>

      <form className="panel settings-form" onSubmit={submit}>
        <label>
          <span>{t("settings.provider")}</span>
          <select
            value={draft.provider}
            onChange={(event) => setDraft({ ...draft, provider: event.target.value })}
          >
            <option value="mock">{t("settings.providerMock")}</option>
            <option value="openai">{t("settings.providerOpenai")}</option>
            <option value="cli">{t("settings.providerCli")}</option>
          </select>
        </label>

        {mode === "openai" ? (
          <div className="settings-provider-section">
            <div className="settings-grid">
              <label>
                <span>{t("settings.openaiBaseUrl")}</span>
                <input
                  required
                  value={draft.openai_base_url ?? ""}
                  onChange={(event) => setDraft({ ...draft, openai_base_url: event.target.value })}
                />
              </label>
              <label>
                <span>{t("settings.openaiModel")}</span>
                <input
                  required
                  value={draft.openai_model ?? ""}
                  onChange={(event) => setDraft({ ...draft, openai_model: event.target.value })}
                />
              </label>
              <label>
                <span>{t("settings.openaiApiKey")}</span>
                <input
                  type="password"
                  value={draft.openai_api_key ?? ""}
                  placeholder={t("settings.openaiApiKeyPlaceholder")}
                  onChange={(event) => setDraft({ ...draft, openai_api_key: event.target.value })}
                />
                <em className="field-help">
                  {settings.data?.has_openai_api_key ? t("settings.keyConfigured") : t("settings.keyMissing")}
                </em>
              </label>
            </div>
          </div>
        ) : null}

        {mode === "cli" ? (
          <div className="settings-provider-section">
            <div className="settings-grid single-row">
              <label>
                <span>{t("settings.cliPath")}</span>
                <input
                  required
                  value={draft.cli_path ?? ""}
                  onChange={(event) => setDraft({ ...draft, cli_path: event.target.value })}
                />
              </label>
            </div>

            <section className="inline-help">
              <strong>{t("settings.cliLoginCommand")}</strong>
              <code>{settings.data?.cli_login_command ?? "codex login --device-auth"}</code>
              <p>{t("settings.cliHelp")}</p>
            </section>

            <details className="settings-advanced">
              <summary>{t("settings.cliAdvanced")}</summary>
              <div className="settings-grid">
                <label>
                  <span>{t("settings.cliModel")}</span>
                  <input
                    value={draft.cli_model ?? ""}
                    onChange={(event) => setDraft({ ...draft, cli_model: event.target.value })}
                  />
                </label>
                <label>
                  <span>{t("settings.cliProfile")}</span>
                  <input
                    value={draft.cli_profile ?? ""}
                    onChange={(event) => setDraft({ ...draft, cli_profile: event.target.value })}
                  />
                </label>
              </div>
            </details>
          </div>
        ) : null}

        <em className="field-help">{t("settings.localSaveNote")}</em>

        <div className="settings-actions">
          <button className="primary-button" type="submit">
            <Save size={18} />
            {t("settings.save")}
          </button>
          {saved ? <span className="success-copy">{t("settings.saved")}</span> : null}
        </div>
      </form>

      <EmptyState title={t(helpCopy.title)}>{t(helpCopy.body)}</EmptyState>
    </div>
  );
}

function providerHelpCopy(mode: AiProviderMode): { title: TranslationKey; body: TranslationKey } {
  if (mode === "openai") {
    return { title: "settings.providerOpenai", body: "settings.openaiNote" };
  }

  if (mode === "cli") {
    return { title: "settings.providerCli", body: "settings.cliNote" };
  }

  return { title: "settings.providerMock", body: "settings.mockNote" };
}
