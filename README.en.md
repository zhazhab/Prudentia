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
AI_CLI_PROFILE=
```

`AI_PROVIDER` may be an ordered fallback chain such as `cli,openai`. A provider can only fall back before it emits visible response text, and the runtime never falls back to mock automatically. Greetings and ordinary questions also go through the configured real provider instead of hardcoded replies.

Optionally enable Tavily research:

```env
WEB_RESEARCH_PROVIDER=tavily
TAVILY_API_KEY=your_key
```

Use `WEB_RESEARCH_PROVIDER=disabled` when it is not configured. Unavailable external search does not block local-context conversation, but the response is marked as not externally verified. Conversation attachments, company Markdown projections, and SQLite use the original repository's shared local root; the database stores relative paths so all worktrees read and write the same local material.

You can also edit AI settings from the app Settings page. Saving applies changes immediately and writes them to the shared `.env` in the original repository working tree, so the settings persist across backend restarts and new worktrees.

## Import Template

See [examples/portfolio_import.csv](examples/portfolio_import.csv) for the first supported portfolio format.
