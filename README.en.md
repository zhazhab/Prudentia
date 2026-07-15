# Prudentia

[中文](README.md)

Prudentia is a local-first investment workspace. The current frontend opens on a chat-first investment memo home and focuses on portfolio holdings/import visibility, memo management, and local AI provider configuration.

## Repository Name

`Prudentia` draws from prudence and practical wisdom: careful judgment, disciplined action, and the ability to make better decisions under uncertainty. The name emphasizes discipline, review, and long-termism instead of short-term trading impulse.

## Vision

Prudentia aims to become a local-first investment operating system for individual investors. It helps turn scattered research, investment decisions, portfolio feedback, and self-observation into reviewable assets, so each investment action can compound into long-term capability.

## Ideal Goal

In the ideal state, users can build their own investment system in Prudentia and record the assumptions, risks, catalysts, disconfirming evidence, and review results behind every decision. The system gradually forms an RPG-like investor profile from those behaviors, helping users understand their circle of competence, decision discipline, risk preferences, and recurring biases.

## Current Capabilities

- Rust backend with `axum`, `sqlx`, SQLite, provider-based AI, and provider-based market data.
- React + Vite + TypeScript frontend with a chat-first home, Portfolio, Memos, and Settings views.
- Chat-first home with natural replies from real AI providers, replayable persisted run events, one send/stop control, attachments and research sources, independently confirmable data actions, and portfolio/company/used-context side panels; threads and context become drawers on mobile.
- Portfolio CSV/Excel/screenshot unified draft import, field mapping, local code-directory matching, confirmed merge commit, position edit/delete, value/weight/P/L and return-rate calculations, holdings-table sorting, automatic trade adjustments, CNY base summary, holding snapshot returns and portfolio time-weighted return views, index-proxy comparison, ISO currency money display, and daily-TTL/manual forced quote and FX refresh.
- Memo workflow for creating notes and using AI extraction for thesis, risks, catalysts, disconfirming evidence, and checklist items.
- AI Settings page for explicit Mock, OpenAI-compatible, and CLI providers; Codex CLI is the default, ordered fallback chains only switch before visible text is emitted, and settings are saved to the original repository working tree's shared `.env`.
- English and Simplified Chinese UI, with `Accept-Language` passed to backend-generated system text.

## Planned Capabilities

- A fuller memo lifecycle: watch, buy, add, trim, sell, review, and archive.
- Stronger portfolio import flows: saved mappings, duplicate import handling, and account/market/sector analysis.
- More market data, AI, and CLI providers behind clean replacement interfaces.
- Decision review reminders that turn review dates, decision-delta snapshots, and thesis horizons into an actionable workflow.
- Reconnect the Research Library, Investment System, and Profile to the frontend.
- Expanded investor profile rules so XP, attributes, badges, and bias signals better reflect the user's investment process.
- Reserved broker and transaction sync interfaces while keeping local-first storage and replaceable provider boundaries.
- Exportable investment systems, memos, and review reports for long-term archiving and sharing.

## Repository Layout

```text
backend/   Rust API service
frontend/  React application
docs/      Architecture, API, and import notes
examples/  Sample portfolio import files
```

See [docs/engineering-style.en.md](docs/engineering-style.en.md) for code style and maintainability expectations.

See [CHANGELOG.en.md](CHANGELOG.en.md) for notable project changes. Every development change should update the changelog; update this README when setup, capabilities, public interfaces, or common workflows change.

## Backend

```sh
cp .env.example .env
cargo run -p prudentia-backend
```

The backend listens on `http://127.0.0.1:8080` by default and stores local data in `data/prudentia.sqlite`.
When starting both services with `./scripts/dev.sh` or `make start`, the script automatically selects the next available port if a default port is occupied and prints the actual URLs.

Useful commands:

```sh
cargo fmt
make check-backend-size
make check-backend-clippy
cargo test -p prudentia-backend
```

## Frontend

```sh
npm install --prefix frontend
npm --prefix frontend run dev
```

The frontend dev server listens on `http://127.0.0.1:5173` and proxies `/api` to the backend.
When started through `./scripts/dev.sh`, the frontend is wired to the backend port chosen by the script.

Useful commands:

```sh
npm --prefix frontend run build
```

## Local Config And Data

The backend reads local `.env` and SQLite state from the original repository working tree behind the Git common dir by default, so launching from different git worktrees reads and writes the same original-repo configuration and portfolio data. Relative SQLite URLs such as `DATABASE_URL=sqlite://data/prudentia.sqlite` resolve relative to the original repository working tree instead of the current worktree.

To override the local state directory:

```env
PRUDENTIA_LOCAL_DIR=.prudentia-local
DATABASE_URL=sqlite://data/prudentia.sqlite
```

When the AI Settings page persists settings locally, it also writes to this shared `.env`.

## Provider Defaults

`AI_PROVIDER=mock` and `MARKET_DATA_PROVIDER=mock` make the app runnable without external API keys; mock quotes are for offline development and do not update real holding or benchmark returns.

`MARKET_DATA_PROVIDER` accepts a comma-separated fallback chain. Supported entries are `mock`, `yahoo`, `tencent`, `longbridge`, and `alpha_vantage`. Yahoo and Tencent quotes do not require API keys:

```env
MARKET_DATA_PROVIDER=yahoo,tencent
PRICE_REFRESH_TTL_SECS=86400
```

To use Longbridge OpenAPI quotes:

```env
MARKET_DATA_PROVIDER=longbridge,yahoo
LONGBRIDGE_APP_KEY=your_app_key
LONGBRIDGE_APP_SECRET=your_app_secret
LONGBRIDGE_ACCESS_TOKEN=your_access_token
PRICE_REFRESH_TTL_SECS=86400
```

Longbridge credentials are read from environment variables by the official SDK. `LONGBRIDGE_ACCESS_TOKEN` may carry account or trading permissions, so keep it in your local `.env` and do not commit it.

To use Alpha Vantage-compatible quote refreshes:

```env
MARKET_DATA_PROVIDER=alpha_vantage
ALPHA_VANTAGE_API_KEY=your_key
PRICE_REFRESH_TTL_SECS=86400
```

Quotes, FX, and benchmark indexes refresh automatically with a 24-hour TTL by default, and the holdings page can trigger one manual forced refresh. Forced refreshes bypass the daily TTL, but still go through provider-level throttling, 429/rate-limit cooldowns, and the fallback chain; Tencent quote and Longbridge providers batch same-cycle holding/benchmark quote requests. Refresh failures keep stale state and log warnings without blocking startup.

The local security code directory uses the no-account public provider by default for screenshot or draft name-to-`symbol` matching:

```env
SYMBOL_DIRECTORY_PROVIDER=public
SYMBOL_DIRECTORY_PUBLIC_CONFIG=config/symbol-directory-public.json
SYMBOL_DIRECTORY_REFRESH_INTERVAL_SECS=86400
```

The public provider reads the normalized in-repo inventory file `data/symbol-directory/public/symbols.json` by default and imports it into the local SQLite code directory at startup. That file is generated from the public directories declared in `config/symbol-directory-public.json`, including SSE stock/fund suggestion data, HKEX English and Traditional Chinese securities lists, and Nasdaq Trader US symbol lists. It does not require an account or token. Each security record in the inventory only keeps `symbol`, `name`, `market`, and `currency`; SQLite `security_symbols` stores the same four fields plus the file-level `updated_at`. Before writing the inventory, Traditional Chinese security names are converted to Simplified Chinese. On backend startup, Prudentia checks the inventory `updated_at`; it reuses the file for 24 hours by default, and only refreshes public sources asynchronously after expiry before replacing that inventory file. Refresh failures only emit warnings and do not block startup or existing local matching. Imports only query the local SQLite directory and do not make live external search requests. Chinese-name matching folds Traditional and Simplified variants; when matching still fails, fill `symbol` manually in the draft or switch to a future authorized provider.

Optionally, use Tushare to refresh the local security code directory:

```env
SYMBOL_DIRECTORY_PROVIDER=tushare
TUSHARE_TOKEN=your_token
SYMBOL_DIRECTORY_REFRESH_INTERVAL_SECS=86400
```

To use an OpenAI-compatible chat completions endpoint:

```env
AI_PROVIDER=openai
OPENAI_API_KEY=your_key
OPENAI_BASE_URL=https://api.openai.com/v1
OPENAI_MODEL=gpt-4.1-mini
OPENAI_MODEL_SIMPLE=gpt-4.1-mini
OPENAI_MODEL_STANDARD=gpt-4.1-mini
OPENAI_MODEL_DEEP=gpt-4.1-mini
```

OpenAI-compatible conversations use real SSE token streaming. To use the generic CLI provider with Codex CLI and ChatGPT/device-code authentication:

```sh
codex login --device-auth
```

Then set:

```env
AI_PROVIDER=cli
AI_CLI_PROVIDER=codex
AI_CLI_PATH=codex
AI_CLI_MODEL=
AI_CLI_MODEL_SIMPLE=gpt-5.6-luna
AI_CLI_MODEL_STANDARD=gpt-5.6-terra
AI_CLI_MODEL_DEEP=gpt-5.6-sol
AI_CLI_PROFILE=
```

`AI_PROVIDER` may be an ordered fallback chain such as `cli,openai`. A provider can only fall back before it emits visible response text, the runtime never falls back to mock automatically, and saving model tiers in Settings preserves the existing fallback chain. Greetings and ordinary questions also go through the configured real provider instead of hardcoded replies. Before model invocation, deterministic rules classify the turn as light, standard, or deep: greetings and short questions prefer the light model, normal company/portfolio discussion uses standard, and attachments, multi-step work, financial or operating-disconfirmation analysis, and investment-rule changes use deep. The frontend shows the current phase, level, actual model, and routing reason. Legacy `AI_CLI_MODEL` or `OPENAI_MODEL` still pins a compatible single-model fallback; tier variables override their matching levels.

Prudentia runs Codex CLI as an ephemeral, isolated, tool-disabled generation process. External research is owned by the application's `WebResearchProvider`; the CLI only reads selected, bounded conversation context and generates the response, without inspecting the workspace, opening a browser, or spawning extra agents. The runtime reports research-cache lookup, retrieval, verification, AI reading/analysis/writing, and confirmation-card extraction as persisted stages. The CLI has no real token deltas and still carries Codex agent startup overhead; use the OpenAI-compatible provider for lower time-to-first-token and true streaming. Visible-response hard limits are 90, 240, and 600 seconds for light, standard, and deep tasks respectively. Confirmation-card extraction allows 120 seconds after light/standard turns and 300 seconds after deep turns. Normal model completion and user cancellation still end the task immediately.

By default, research uses stable public data sources. It requires no search API key and does not depend on the AI CLI's search capabilities:

```env
WEB_RESEARCH_PROVIDER=public_sources
```

Substantive company discussion such as "latest earnings," "analyze PDD," or "what is PDD's moat?" automatically runs three deterministic retrievals: recent SEC filings plus their actual primary filing/earnings attachment, Yahoo Finance company-operating news, and TradingView ideas that have the platform's hot flag, visible engagement, and an operating argument. An explicit request to use only existing/local context skips retrieval and reuses the confirmed company view directly. Broad company analysis explains users, customers, payers, the value chain, and money flows first, then assesses competition, moat, owner economics, management and capital allocation, financial resilience, and company quality. SEC filing extraction selects business, competition, monetization, profit-engine, owner-economics, management/incentive/capital-allocation, and resilience evidence windows. Multi-year SEC Company Facts series include revenue, gross profit, cost of revenue, operating income, net income, selling and marketing expense, operating cash flow, capex, share compensation, diluted weighted-average shares, a free-cash-flow proxy, and its per-share proxy. The proxy is operating cash flow minus total capex and is never mislabeled as owner earnings when maintenance and growth capex cannot be separated. The `ResearchPlan` contains only the company, structured intent, normalized period, and bounded official/independent/community queries; providers never receive raw conversation text or position sizes. Response context retains up to three sources per tier, with up to 8,000 characters per primary source and 2,500 per independent/community source; persistence remains bounded at nine sources per turn and 8,000 characters per source. Complete and partial results are cached for 24 hours, but only compact summaries and source URLs are stored locally, never raw Company Facts JSON.

Company threads currently analyze the enterprise only. Home opens the latest active thread after reload. Confirmed company views are stored as local versions and shown by default in the Company View tab of every thread bound to that company. When a bound company thread explicitly asks about another held company, the thread binding stays unchanged while research, sources, company context, and turn summaries switch to the explicitly named company; the bound company's summaries, view, and research cache are not reused. A company short name resolves automatically only when holdings, the bound company, a security code, and the local symbol directory produce one high-confidence candidate. A lowercase pure-letter ticker must also sit next to explicit investment, analysis, research, or company-question wording, preventing greeting words from becoming securities. Multiple or unknown candidates trigger a request for the full company name or security code; no research runs and the thread company is never used as a fallback before confirmation. The original request and candidates are persisted with the clarification message, so replying with a security code or an ordinal such as “the first one” resumes the request even after refresh. A new complete request supersedes that clarification; a still-ambiguous selection asks again. Context sent to the response model excludes portfolio summaries, positions, personal trade history, and the investment system. Share prices, quotes, market capitalization, valuation multiples, price targets, stock returns, technical analysis, ratings, and personal P/L are ignored even when they occur in prior messages or sources. Product markets, industry structure, competitors, and market share remain in scope. Replies end with company quality and operating uncertainty, without valuation, a security view, portfolio impact, or trading conclusions. Automatic company-view patches leave `valuation_expectations` unset, and `thesis` means the operating company thesis only.

Moat analysis uses a reverse audit. Market share, product quality, management/execution, founder talent, marketing/distribution, and temporary technology leadership are outcomes or capabilities rather than moat proof. The system tests candidate mechanisms such as brand pricing power, network effects, scale economies, switching costs, protected IP, and exclusive resources or licenses; it must explain how each constrains competition and protects excess returns, then test subsidy removal, rival replication, founder departure, and channel or technology shifts. Outputs distinguish temporary advantage, medium-term capability, and durable structural moat, with strength, evidence, and kill conditions.

A focused moat analysis cannot substitute a scorecard or threat list for reasoning. Risk, competition, counterevidence, and "how could a competitor breach it" remain parts of the moat audit instead of switching the reply to a broad company report. Every material mechanism must provide dated facts and sources, the full profit-protection chain, a competitor or outside-view comparison, maintenance cost, counterevidence and alternative explanations, confidence, and why the verdict is not one level higher or lower. A 1-5 score is forbidden without observable anchors for 1, 3, and 5. Breach analysis expands only the three to five paths ranked highest by probability times impact. Every path repeats the same explicit fields for starting assets, cheapest sequence, required capital/time/capabilities, likely company response, transmission into retention/take rate/contribution margin/economic profit, leading indicators, disconfirming condition, missing evidence, and verdict-change point; when space is tight, it includes fewer paths instead of truncating the last one. The proposed company view preserves the same fields instead of reducing them to a risk summary; the moat section is capped at 3,500 characters and one company-view patch at 9,000 total.

Business-model analysis is no longer a short opening inside a broad report. Broad company analysis targets roughly 4,000-5,000 high-density Simplified Chinese characters and reserves at least 60% of substantive content for the business model; a focused business-model response targets 3,500-4,500 characters with at least 75% reserved for the core. It traces the offering and customer job, transaction lifecycle and risk ownership, monetization and profit pool, segment-level unit economics, competitors and attack paths, positive compounding conditions, and inverse failure plus multidisciplinary synthesis. Materially different products, geographies, and fulfillment models cannot be hidden in consolidated averages, and every section distinguishes verified facts, reasoned inference, and unknowns. The positive case states the conditions that must hold together, reinforcing loops, and incremental economics. The inverse case works backward from economic-profit collapse or permanent loss, including subsidy/cheap-capital dependence, channel/regulatory/key-person/counterparty risks, bottlenecks, feedback reversals, and second-order cascades that can occur even while revenue grows. The system selects only three to five multidisciplinary lenses with causal explanatory power, and each must state the observed mechanism, required evidence, and implication. The final verdict distinguishes genuine value creation from value transfer, rates the model fragile/mixed/robust, and names earliest break signals and kill conditions.

Business-model, moat, and broad company analysis answer the six investor questions only after a knowability gate. The system classifies the enterprise as predictable, partially predictable, or not predictably bounded; names three to five decisive operating variables; and tests historical stability, industry or technology change, cyclicality, regulation, management dependence, and evidence quality. A normalized financial base is necessary but not sufficient. Five- and ten-year company-operating downside/base/upside profit or owner-earnings ranges are calculated only when the gate passes; otherwise the response gives a qualitative scenario architecture, exact blockers, and evidence needed to quantify later. The shared audit also covers owner earnings and diluted per-share economics, maintenance versus growth capex, dilution, incremental ROIC, return on retained earnings, reinvestment runway and duration, great/good/gruesome economics, management ability/integrity/candor, succession, stakeholder incentives, historical capital allocation, and ruin risks. The six-row matrix still presents facts, optimistic and pessimistic cases, current verdict, and missing evidence. Scenarios concern company operations only, never stock-market bear/bull conditions, share prices, or valuation multiples.

Tavily remains available as an alternative:

```env
WEB_RESEARCH_PROVIDER=tavily
TAVILY_API_KEY=your_key
```

Set `WEB_RESEARCH_PROVIDER=disabled` explicitly to turn automatic research off. Unavailable external search does not block local-context conversation, but the response is marked as not externally verified. Conversation attachments, company Markdown projections, and SQLite use the original repository's shared local root; the database stores relative paths so all worktrees read and write the same local material. To bound long-term usage, streaming deltas exist only while a run is active and the final message replaces them at terminal state; research cache rows are physically removed after 24 hours. Investment messages, sources, confirmation actions, company/trade/rule versions, and portfolio snapshots are never automatically deleted. Freed SQLite pages remain reusable inside the file instead of running an expensive `VACUUM` after every turn.

You can also edit AI settings from the app Settings page. Saving applies changes immediately and writes them to the shared `.env` in the original repository working tree, so the settings persist across backend restarts and new worktrees.

## Import Template

See [examples/portfolio_import.csv](examples/portfolio_import.csv) for the first supported portfolio format.
