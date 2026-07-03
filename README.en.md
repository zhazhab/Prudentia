# Prudentia

[中文](README.md)

Prudentia is a local-first investment workspace for personal investment systems, thesis-driven memos, portfolio visibility, and RPG-like investor profiling.

## Repository Name

`Prudentia` draws from prudence and practical wisdom: careful judgment, disciplined action, and the ability to make better decisions under uncertainty. The name emphasizes discipline, review, and long-termism instead of short-term trading impulse.

## Vision

Prudentia aims to become a local-first investment operating system for individual investors. It helps turn scattered research, investment decisions, portfolio feedback, and self-observation into reviewable assets, so each investment action can compound into long-term capability.

## Ideal Goal

In the ideal state, users can build their own investment system in Prudentia and record the assumptions, risks, catalysts, disconfirming evidence, and review results behind every decision. The system gradually forms an RPG-like investor profile from those behaviors, helping users understand their circle of competence, decision discipline, risk preferences, and recurring biases.

## Current Capabilities

- Rust backend with `axum`, `sqlx`, SQLite, provider-based AI, and provider-based market data.
- React + Vite + TypeScript frontend with Dashboard, Portfolio, Decision Deltas, Memos, Investment System, and Profile views.
- Portfolio CSV/Excel/screenshot unified draft import, field mapping, confirmed merge commit, position edit/delete, value/weight/P/L calculations, CNY base summary, and scheduled quote + FX refresh.
- Decision Deltas turn quantified decisions into actual legs plus baseline shadow legs, refresh market-data snapshots, and show portfolio return differences, one-fork comparisons, and review candidate adoption.
- Research Library for article/person investment-thought distillation, stock snapshot analysis, portfolio reviews, and adopting candidate principles/checklist items into the investment system.
- Rule-driven profile XP, levels, badges, attributes, and bias signals.
- English and Simplified Chinese UI, with `Accept-Language` passed to backend-generated system text.

## Planned Capabilities

- A fuller memo lifecycle: watch, buy, add, trim, sell, review, and archive.
- Stronger portfolio import flows: saved mappings, duplicate import handling, and account/market/sector analysis.
- More market data, AI, and CLI providers behind clean replacement interfaces.
- Decision review reminders that turn review dates, decision-delta snapshots, and thesis horizons into an actionable workflow.
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

## Provider Defaults

`AI_PROVIDER=mock` and `MARKET_DATA_PROVIDER=mock` make the app runnable without external API keys.

To use Alpha Vantage-compatible quote refreshes:

```env
MARKET_DATA_PROVIDER=alpha_vantage
ALPHA_VANTAGE_API_KEY=your_key
```

To use an OpenAI-compatible chat completions endpoint:

```env
AI_PROVIDER=openai
OPENAI_API_KEY=your_key
OPENAI_BASE_URL=https://api.openai.com/v1
OPENAI_MODEL=gpt-4.1-mini
```

To use the generic CLI provider with Codex CLI and ChatGPT/device-code authentication:

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

You can also edit AI settings from the app Settings page. Runtime changes take effect immediately; enabling "write .env" persists them for backend restarts.

## Import Template

See [examples/portfolio_import.csv](examples/portfolio_import.csv) for the first supported portfolio format.
