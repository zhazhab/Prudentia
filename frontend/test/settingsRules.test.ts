import assert from "node:assert/strict";
import test from "node:test";
import {
  buildAiSettingsPayload,
  providerSettingsView,
  shouldExpandCliAdvanced
} from "../src/pages/settingsRules.ts";

test("mock settings persist only the selected provider", () => {
  const view = providerSettingsView("mock");
  const payload = buildAiSettingsPayload({
    provider: "mock",
    openai_base_url: "https://api.openai.com/v1",
    openai_model: "gpt-4.1-mini",
    openai_api_key: "sk-test",
    cli_provider: "codex",
    cli_path: "codex",
    cli_model: "gpt-5",
    cli_profile: "work"
  });

  assert.equal(view.showOpenAi, false);
  assert.equal(view.showCli, false);
  assert.equal(view.helpTitleKey, "settings.providerMock");
  assert.deepEqual(payload, {
    provider: "mock",
    persist_to_env: true
  });
});

test("openai settings persist only openai fields and omit blank api keys", () => {
  const view = providerSettingsView("openai");
  const payload = buildAiSettingsPayload({
    provider: "openai",
    openai_base_url: "https://api.example.com/v1",
    openai_model: "gpt-4.1-mini",
    openai_api_key: "   ",
    cli_provider: "codex",
    cli_path: "codex",
    cli_model: "gpt-5",
    cli_profile: "work"
  });

  assert.equal(view.showOpenAi, true);
  assert.equal(view.showCli, false);
  assert.equal(view.helpTitleKey, "settings.providerOpenai");
  assert.deepEqual(payload, {
    provider: "openai",
    openai_base_url: "https://api.example.com/v1",
    openai_model: "gpt-4.1-mini",
    persist_to_env: true
  });
});

test("cli settings persist only cli fields", () => {
  const view = providerSettingsView("cli");
  const payload = buildAiSettingsPayload({
    provider: "cli",
    openai_base_url: "https://api.openai.com/v1",
    openai_model: "gpt-4.1-mini",
    openai_api_key: "sk-test",
    cli_provider: "codex",
    cli_path: "/opt/homebrew/bin/codex",
    cli_model: "gpt-5",
    cli_profile: "work"
  });

  assert.equal(view.showOpenAi, false);
  assert.equal(view.showCli, true);
  assert.equal(view.helpTitleKey, "settings.providerCli");
  assert.deepEqual(payload, {
    provider: "cli",
    cli_provider: "codex",
    cli_path: "/opt/homebrew/bin/codex",
    cli_model: "gpt-5",
    cli_profile: "work",
    persist_to_env: true
  });
});

test("cli advanced settings expand when existing values are configured", () => {
  assert.equal(shouldExpandCliAdvanced("", ""), false);
  assert.equal(shouldExpandCliAdvanced("gpt-5", ""), true);
  assert.equal(shouldExpandCliAdvanced("", "work"), true);
});
