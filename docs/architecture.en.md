# Prudentia Architecture

[中文](architecture.md)

## Shape

Prudentia is a local-first monorepo:

- `backend`: Rust API service using Axum, Tokio, SQLx, and SQLite.
- `frontend`: React + Vite + TypeScript workbench.
- `docs`: product and implementation notes.
- `examples`: import templates and sample data.

The backend owns persistence and all provider integrations. The frontend reads files in the browser, sends import content to the backend for preview/commit, and renders portfolio/memo/profile workflows.

Engineering style is documented in [engineering-style.en.md](engineering-style.en.md). Readability, maintainability, and explainability are treated as architecture constraints, not cosmetic preferences.

## Backend Modules

- `memo`: thesis notes, risks, catalysts, disconfirming evidence, tags, and memo AI extraction.
- `research`: local research records, article/person thought distillation, stock snapshots, portfolio reviews, and adoption of candidate principles/checklist items.
- `investment_system`: personal principles, checklist, competence boundaries, and decision rules.
- `portfolio`: import preview, mapping, commit, position calculations, summaries, and refresh orchestration.
- `market_data`: quote provider trait with mock and Alpha Vantage-compatible implementations.
- `decision`: explicit investment decision events.
- `decision_delta`: actual legs and baseline shadow legs for quantifiable decisions, with daily/manual snapshots, stale fallback, reviews, and candidate adoption.
- `profile`: rule-driven XP, levels, attributes, badges, and bias signals.
- `ai`: provider trait with mock, OpenAI-compatible, and CLI-backed implementations.
- `settings`: runtime AI provider configuration with optional `.env` persistence.

## Local-First Defaults

SQLite is the first persistence layer. There is no login, multi-user authorization, or broker API sync in v1. Portfolio quantity and average cost come from import/manual updates; automatic updates only refresh prices and derived values.

Decision Delta v1 does not build an unbounded world tree. Each quantifiable decision creates one actual/baseline fork, then snapshots preserve how that fork performs on later dates. The timeline summary is the sum of latest `actual_value - baseline_value` snapshots in the current filter scope, not a full counterfactual portfolio NAV.

## Extension Points

- Add broker sync by introducing a `BrokerProvider` module that writes normalized transaction events.
- Add richer AI workflows by expanding `AiProvider` with memo critique, decision review, and profile narration.
- Add more market data providers behind the existing `MarketDataProvider` trait.
- Add additional AI providers behind the existing `AiProvider` trait. CLI-backed providers share a reusable runner and per-tool backend enum; the current `codex` backend is intentionally implemented through `codex exec` so Codex device-code authentication remains owned by the Codex CLI.
