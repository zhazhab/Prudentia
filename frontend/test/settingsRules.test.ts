import assert from "node:assert/strict";
import test from "node:test";
import { aiSettingsPayload, providerMode } from "../src/pages/settingsRules.ts";
import type { UpdateAiSettings } from "../src/types/domain.ts";

test("provider mode defaults unknown values to the real CLI provider", () => {
  assert.equal(providerMode("mock"), "mock");
  assert.equal(providerMode("openai"), "openai");
  assert.equal(providerMode("cli"), "cli");
  assert.equal(providerMode("codex"), "cli");
  assert.equal(providerMode("codex_cli"), "cli");
  assert.equal(providerMode("unknown"), "cli");
  assert.equal(providerMode(undefined), "cli");
});

test("saving model tiers preserves an existing provider fallback chain", () => {
  const payload = aiSettingsPayload(
    draft({ provider: "cli" }),
    ["cli", "openai"]
  );

  assert.equal(payload.provider, "cli,openai");
});

test("choosing a different provider intentionally replaces the fallback chain", () => {
  const payload = aiSettingsPayload(
    draft({ provider: "openai" }),
    ["cli", "openai"]
  );

  assert.equal(payload.provider, "openai");
});

test("mock settings persist locally without unrelated provider fields", () => {
  assert.deepEqual(aiSettingsPayload(draft({ provider: "mock" })), {
    provider: "mock",
    persist_to_env: true
  });
});

test("openai settings persist locally with only openai fields", () => {
  assert.deepEqual(
    aiSettingsPayload(
      draft({
        provider: "openai",
        openai_base_url: " https://openrouter.ai/api/v1 ",
        openai_model: " gpt-4.1-mini ",
        openai_api_key: " sk-test ",
        cli_path: "custom-codex"
      })
    ),
    {
      provider: "openai",
      persist_to_env: true,
      openai_base_url: "https://openrouter.ai/api/v1",
      openai_model: "gpt-4.1-mini",
      openai_model_simple: "gpt-4.1-mini",
      openai_model_standard: "gpt-4.1-mini",
      openai_model_deep: "gpt-4.1-mini",
      openai_api_key: "sk-test"
    }
  );
});

test("openai settings omit blank api key so the current key is preserved", () => {
  assert.deepEqual(
    aiSettingsPayload(
      draft({
        provider: "openai",
        openai_api_key: " "
      })
    ),
    {
      provider: "openai",
      persist_to_env: true,
      openai_base_url: "https://api.openai.com/v1",
      openai_model: "gpt-4.1-mini",
      openai_model_simple: "gpt-4.1-mini",
      openai_model_standard: "gpt-4.1-mini",
      openai_model_deep: "gpt-4.1-mini"
    }
  );
});

test("cli settings persist locally with only cli fields", () => {
  assert.deepEqual(
    aiSettingsPayload(
      draft({
        provider: "cli",
        cli_path: " /opt/homebrew/bin/codex ",
        cli_model: " gpt-5 ",
        cli_profile: " personal ",
        openai_api_key: "sk-test"
      })
    ),
    {
      provider: "cli",
      persist_to_env: true,
      cli_provider: "codex",
      cli_path: "/opt/homebrew/bin/codex",
      cli_model_simple: "gpt-5",
      cli_model_standard: "gpt-5",
      cli_model_deep: "gpt-5",
      cli_profile: "personal"
    }
  );
});

test("cli settings preserve distinct adaptive model tiers", () => {
  assert.deepEqual(
    aiSettingsPayload(
      draft({
        provider: "cli",
        cli_model_simple: " gpt-5.6-luna ",
        cli_model_standard: " gpt-5.6-terra ",
        cli_model_deep: " gpt-5.6-sol "
      })
    ),
    {
      provider: "cli",
      persist_to_env: true,
      cli_provider: "codex",
      cli_path: "codex",
      cli_model_simple: "gpt-5.6-luna",
      cli_model_standard: "gpt-5.6-terra",
      cli_model_deep: "gpt-5.6-sol",
      cli_profile: ""
    }
  );
});

function draft(overrides: UpdateAiSettings = {}): UpdateAiSettings {
  return {
    provider: "mock",
    openai_base_url: "https://api.openai.com/v1",
    openai_model: "gpt-4.1-mini",
    openai_api_key: "",
    cli_provider: "codex",
    cli_path: "codex",
    cli_model: "",
    cli_profile: "",
    persist_to_env: false,
    ...overrides
  };
}
