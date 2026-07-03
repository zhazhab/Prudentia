# Changelog

[中文](CHANGELOG.md)

All notable changes to Prudentia should be recorded here. Add the newest entry at the top of the current release section.

## Unreleased

- Improved the development startup script: `./scripts/dev.sh` now chooses available ports when defaults are occupied and wires the frontend to the selected backend URL.
- Improved AI settings: the Settings page now shows only the fields needed for Mock, OpenAI-compatible, or CLI provider modes, and saving writes the configuration to the local `.env` by default.
- Optimized Decision Delta performance: timeline now uses batched reads, detail snapshots default to the latest 90 entries, refresh reuses quote/FX lookups, key SQLite indexes were added, and the symbol filter is debounced.
- Added the Decision Delta Timeline: buy/add, sell/trim, and watch/skip decisions can create actual legs plus baseline shadow legs, refresh snapshots from current market data, and show visible return-delta summaries, one-fork comparisons, review candidate adoption, and process-based profile rewards.
- Polished the Portfolio holdings workflow: CSV/Excel/screenshot inputs now share one editable draft table, confirmed drafts merge-upsert by `symbol`, positions can be edited/deleted, US/HK/CN markets are inferred, summaries use a CNY base view, and market data providers refresh FX with stale fallback.
- Added the Research Library with article/person investment-thought distillation, stock snapshot analysis, portfolio reviews, and adoption of candidate principles/checklist items into the investment system.
- Added repository name explanation, vision, ideal goals, and planned capabilities to the README.
- Split bilingual documentation into separate files: Simplified Chinese stays in `.md`, and English uses `.en.md`.
- Made the changelog available in English and Simplified Chinese.
- Added Simplified Chinese alongside English throughout the README.
- Documented changelog ordering: newest entries should be inserted at the top of the current release section.
- Documented the development completion rule: update this changelog after every development change and update the README when setup, capabilities, public interfaces, or workflows change.
- Added engineering style guidance covering readability, maintainability, explainability, Rust design practices, generics, enums, traits, comments, and review expectations.
- Added Codex CLI support as the first CLI AI backend, including device-code login guidance through `codex login --device-auth`.
- Added provider-based AI and market data boundaries, including mock providers, OpenAI-compatible AI, Alpha Vantage-compatible market data, and a reusable CLI AI provider layer.
- Scaffolded the local-first Prudentia workspace with Rust backend, SQLite persistence, React + Vite frontend, portfolio import, memo workflows, investment system editing, profile feedback, and bilingual UI.
