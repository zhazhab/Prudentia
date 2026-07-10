# Prudentia Architecture

[中文](architecture.md)

## Shape

Prudentia is a local-first monorepo:

- `backend`: Rust API service using Axum, Tokio, SQLx, and SQLite.
- `frontend`: React + Vite + TypeScript workbench.
- `docs`: product and implementation notes.
- `examples`: import templates and sample data.

The backend owns persistence and all provider integrations. The frontend reads files in the browser, sends import content to the backend for preview/commit, and renders the currently wired chat-first memo home, portfolio, memo, and AI settings workflows.

Engineering style is documented in [engineering-style.en.md](engineering-style.en.md). Readability, maintainability, and explainability are treated as architecture constraints, not cosmetic preferences.

## Backend Modules

- `memo`: thesis notes, risks, catalysts, disconfirming evidence, tags, and memo AI extraction.
- `conversation`: the deep `ConversationEngine` module that owns subject resolution, progressive context, external research, real model calls, task lifecycle, durable events, and structured action proposals.
- `memo_thread`: low-level thread/message persistence, pagination, archive, and soft delete.
- `research`: local research records, article/person thought distillation, stock snapshots, portfolio reviews, and adoption of candidate principles/checklist items.
- `investment_system`: versioned executable DAGs, the fixed-rule kernel, skill/agent adapters, JSON Schema validation, and execution traces; the old natural-language system remains migration material only.
- `portfolio`: import preview, mapping, local security symbol directory, holding baselines, immutable trade/reversal ledger, position projections, TWR, summaries, and refresh orchestration.
- `market_data`: quote/FX provider trait with mock, Yahoo Finance, Tencent quote, Longbridge OpenAPI, and Alpha Vantage-compatible implementations, plus comma-configured fallback chains, provider-level throttling/cooldowns, and Tencent/Longbridge batch quotes.
- `decision`: explicit investment decision events.
- `decision_delta`: actual legs and baseline shadow legs for quantifiable decisions, with daily/manual snapshots, stale fallback, reviews, and candidate adoption.
- `profile`: rule-driven XP, levels, attributes, badges, and bias signals.
- `ai`: provider trait with mock, OpenAI-compatible, and CLI-backed implementations.
- `settings`: runtime AI provider configuration persisted to the shared `.env` in the original repository working tree.

## Local-First Defaults

Local `.env`, the default SQLite database, attachment originals, and company Markdown projections are read from the original repository working tree behind the Git common dir by default, so different git worktrees read and write the same configuration and investment data. `PRUDENTIA_LOCAL_DIR` can override this directory; relative SQLite URLs resolve from that local state directory. Attachments and company projections live under `data/workspace`, and the database stores relative paths only.

SQLite is the first persistence layer. There is no login, multi-user authorization, or broker API sync in v1. Portfolio quantity and average cost come from import/manual updates; automatic updates only refresh prices and derived values.

Portfolio Performance uses portfolio value snapshots and system-recorded trade adjustments. Import confirmation, draft confirmation, position edits, and position deletes write a `buy` or `sell` adjustment into `portfolio_cash_flows` when the CNY portfolio value changes; daily quote refreshes only write snapshots and never create trade adjustments. Portfolio return compounds each snapshot interval as `(ending value - interval net trade adjustment) / starting value - 1` to produce time-weighted return while also exposing the unadjusted snapshot return for explanation. Deleting down to an empty portfolio becomes the new starting boundary for later performance reads, so cleanup followed by reimport is not miscounted as investment return. The holdings table period return reuses the same `this month` / `this year` / `since inception` selector and computes each holding from its CNY value snapshot change. S&P ETF proxy, Hang Seng ETF proxy, and official SSE Composite snapshots are written only during the holding price-refresh cycle for same-period return comparison. Holding unrealized P/L uses the common broker holdings-page convention: native market value is `last_price × quantity`, native unrealized P/L is `(last_price - average_cost) × quantity`, current return rate is `unrealized_pnl / (average_cost × quantity)`, and CNY totals convert by FX.

Conversation home treats the thread as the default interaction object. Desktop uses thread, primary conversation, and context columns with the conversation taking most space; mobile moves threads and context into left/right drawers. The right panel provides current holdings, the current company view, and context used for the latest response. Send and stop share one control, no empty assistant bubble is created before text exists, and the independent runtime component only displays persisted backend phase, provider, and elapsed time.

Raw messages plus `conversation_runs` and `conversation_run_events` are the facts of record. `ConversationEngine` atomically persists user input and the run before resolving subject, assembling context, researching when needed, generating the natural reply, extracting structured actions, and persisting projections. WebSocket is only an `event_id` cursor replay/subscription channel, so disconnect does not cancel work. Each thread allows one active run while different threads run concurrently, and startup recovery marks abandoned work `interrupted`. Immutable turn summaries, rolling thread summaries, company views, holdings, and rule graphs are rebuildable projections.

Every visible reply comes from the configured real provider, including greetings and capability questions. OpenAI-compatible providers parse real SSE tokens. Codex CLI continuously consumes JSONL run events; when it has no token delta, only phases update and the final body appears once. `AI_PROVIDER` supports ordered fallback, but can only switch before the previous provider emits body text, and mock is never selected automatically. External research uses a `WebResearchProvider` interface with Tavily and disabled adapters; unavailable search continues with local context and explicitly marks external verification incomplete.

The model proposes changes but never writes investment data directly. Company-section patches, trade records, and rule-graph patches become independent confirmation cards. After user edit/confirmation, deterministic domain code executes them with target-version checks and idempotent state. A company confirmation creates an immutable version and Markdown projection. A trade confirmation uses historical FX and the import baseline to update quantity, average cost, and TWR cash flow, while corrections use reversal/replacement events. A rule confirmation creates and activates a new DAG version only after port-type, acyclicity, schema, and adapter validation.

Security code matching uses the local `security_symbols` directory. The default public provider first reads the normalized in-repo inventory file `data/symbol-directory/public/symbols.json` and imports it into SQLite. That file is generated from the no-account public directories declared in `config/symbol-directory-public.json`; current coverage includes SSE stock/fund data, HKEX English/Traditional Chinese securities lists, and Nasdaq Trader US lists. Inventory security records only store `symbol`, `name`, `market`, and `currency`; SQLite `security_symbols` only adds the file-level `updated_at` and no longer stores provider, exchange, or asset type. Traditional Chinese security names are cleaned to Simplified Chinese before the inventory is written. Startup checks the inventory `updated_at`; the file is reused for 24 hours by default, and public sources are refreshed asynchronously only after expiry before replacing the inventory. Refresh failures only emit warnings and do not block startup or existing local matching. Import confirmation and screenshot recognition only query the local directory instead of making live fuzzy-search requests, avoiding provider rate limits and silent guessing. Chinese matching folds Traditional and Simplified variants; authorized sources such as Tushare or broker OpenAPIs can be added as future `SymbolDirectoryProvider` implementations to improve alias and Chinese-name coverage.

Decision Delta v1 does not build an unbounded world tree. Each quantifiable decision creates one actual/baseline fork, then snapshots preserve how that fork performs on later dates. The timeline summary is the sum of latest `actual_value - baseline_value` snapshots in the current filter scope, not a full counterfactual portfolio NAV.

## Extension Points

- Add broker sync by introducing a `BrokerProvider` module that writes normalized transaction events.
- Add richer AI workflows by expanding `AiProvider` with memo critique, decision review, and profile narration.
- Add more market data providers behind the existing `MarketDataProvider` trait.
- Add more `SymbolDirectoryProvider` implementations behind the local security-symbol directory, such as Tushare, Choice, Futu OpenAPI, or other authorized sources.
- Add additional AI providers behind the existing `AiProvider` trait. CLI-backed providers share a reusable runner and per-tool backend enum; the current `codex` backend is intentionally implemented through `codex exec` so Codex device-code authentication remains owned by the Codex CLI.
