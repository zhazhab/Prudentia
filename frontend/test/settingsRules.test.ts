import assert from "node:assert/strict";
import test from "node:test";
import { aiSettingsPayload, providerMode } from "../src/pages/settingsRules.ts";
import type { UpdateAiSettings } from "../src/types/domain.ts";

test("provider mode falls back to mock for unknown values", () => {
  assert.equal(providerMode("mock"), "mock");
  assert.equal(providerMode("openai"), "openai");
  assert.equal(providerMode("cli"), "cli");
  assert.equal(providerMode("codex"), "cli");
  assert.equal(providerMode("codex_cli"), "cli");
  assert.equal(providerMode("unknown"), "mock");
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
      openai_model: "gpt-4.1-mini"
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
      cli_model: "gpt-5",
      cli_profile: "personal"
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
