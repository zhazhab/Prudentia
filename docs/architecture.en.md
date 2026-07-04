# Prudentia Architecture

[中文](architecture.md)

## Shape

Prudentia is a local-first monorepo:

- `backend`: Rust API service using Axum, Tokio, SQLx, and SQLite.
- `frontend`: React + Vite + TypeScript workbench.
- `docs`: product and implementation notes.
- `examples`: import templates and sample data.

The backend owns persistence and all provider integrations. The frontend reads files in the browser, sends import content to the backend for preview/commit, and renders the currently wired portfolio, memo, and AI settings workflows.

Engineering style is documented in [engineering-style.en.md](engineering-style.en.md). Readability, maintainability, and explainability are treated as architecture constraints, not cosmetic preferences.

## Backend Modules

- `memo`: thesis notes, risks, catalysts, disconfirming evidence, tags, and memo AI extraction.
- `research`: local research records, article/person thought distillation, stock snapshots, portfolio reviews, and adoption of candidate principles/checklist items.
- `investment_system`: personal principles, checklist, competence boundaries, and decision rules.
- `portfolio`: import preview, mapping, local security symbol directory, commit, position calculations, summaries, and refresh orchestration.
- `market_data`: quote provider trait with mock and Alpha Vantage-compatible implementations.
- `decision`: explicit investment decision events.
- `decision_delta`: actual legs and baseline shadow legs for quantifiable decisions, with daily/manual snapshots, stale fallback, reviews, and candidate adoption.
- `profile`: rule-driven XP, levels, attributes, badges, and bias signals.
- `ai`: provider trait with mock, OpenAI-compatible, and CLI-backed implementations.
- `settings`: runtime AI provider configuration with optional `.env` persistence.

## Local-First Defaults

SQLite is the first persistence layer. There is no login, multi-user authorization, or broker API sync in v1. Portfolio quantity and average cost come from import/manual updates; automatic updates only refresh prices and derived values.

Portfolio Performance v1 uses a portfolio value snapshot model. Draft confirmation, position edit, delete, and daily quote refreshes write CNY-based portfolio snapshots and store S&P, Hang Seng, and SSE ETF-proxy snapshots for same-period return comparison. It does not adjust for transactions, cash flows, dividends, fees, or splits; stricter money-weighted or flow-adjusted performance should be added later through transaction events and broker/provider boundaries.

Security code matching uses the local `security_symbols` directory. The default public provider first reads the normalized in-repo inventory file `data/symbol-directory/public/symbols.json` and imports it into SQLite. That file is generated from the no-account public directories declared in `config/symbol-directory-public.json`; current coverage includes SSE stock/fund data, HKEX English/Traditional Chinese securities lists, and Nasdaq Trader US lists. Inventory security records only store `symbol`, `name`, `market`, and `currency`; SQLite `security_symbols` only adds the file-level `updated_at` and no longer stores provider, exchange, or asset type. Traditional Chinese security names are cleaned to Simplified Chinese before the inventory is written. Startup checks the inventory `updated_at`; the file is reused for 24 hours by default, and public sources are refreshed asynchronously only after expiry before replacing the inventory. Refresh failures only emit warnings and do not block startup or existing local matching. Import confirmation and screenshot recognition only query the local directory instead of making live fuzzy-search requests, avoiding provider rate limits and silent guessing. Chinese matching folds Traditional and Simplified variants; authorized sources such as Tushare or broker OpenAPIs can be added as future `SymbolDirectoryProvider` implementations to improve alias and Chinese-name coverage.

Decision Delta v1 does not build an unbounded world tree. Each quantifiable decision creates one actual/baseline fork, then snapshots preserve how that fork performs on later dates. The timeline summary is the sum of latest `actual_value - baseline_value` snapshots in the current filter scope, not a full counterfactual portfolio NAV.

## Extension Points

- Add broker sync by introducing a `BrokerProvider` module that writes normalized transaction events.
- Add richer AI workflows by expanding `AiProvider` with memo critique, decision review, and profile narration.
- Add more market data providers behind the existing `MarketDataProvider` trait.
- Add more `SymbolDirectoryProvider` implementations behind the local security-symbol directory, such as Tushare, Choice, Futu OpenAPI, or other authorized sources.
- Add additional AI providers behind the existing `AiProvider` trait. CLI-backed providers share a reusable runner and per-tool backend enum; the current `codex` backend is intentionally implemented through `codex exec` so Codex device-code authentication remains owned by the Codex CLI.
