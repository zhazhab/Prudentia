# Changelog

[中文](CHANGELOG.md)

All notable changes to Prudentia should be recorded here. Add the newest entry at the top of the current release section.

## Unreleased

- Removed the market-allocation panel from the Portfolio main UI; the page now keeps performance, core stats, benchmark comparison, import tools, and the holdings table.
- Simplified the Portfolio holdings view: removed the top quote-status card, the holdings-table `Status/stale` column, and market-allocation stale-FX labels so price freshness is no longer shown in the main UI.
- Changed the Portfolio Performance default view so period return opens in percentage mode while amount remains available as a toggle.
- Simplified the Portfolio top stats: removed the market-group count card and the stale-FX detail from the quote-status card to avoid showing unnecessary quote-freshness status.
- Expanded Portfolio Performance benchmark comparison: added annualized return fields and a top-level annualized return card, and the index-proxy chart now supports cumulative return, annualized return, and relative excess return dimensions.
- Improved the Portfolio Performance line chart: single-snapshot periods now show explicit starting-point markers and helper copy, and the Y-axis domain is calculated dynamically so a single 0% point no longer looks like an empty or oddly scaled chart.
- Added Portfolio Performance snapshots: `this month`, `this year`, `since inception`, and `amount/percent` views now calculate period returns from CNY portfolio value snapshots, compare against S&P/Hang Seng/SSE ETF proxies (`SPY`, `2800.HK`, `510210.SS`), record snapshots after import confirmation/edit/delete/daily quote refresh, and remove explicit frontend refresh buttons in favor of local API refetches.
- Fixed Portfolio draft confirmation when codes are missing: code matching and draft confirmation now first inherit a unique `symbol` from existing holdings by name, market, and currency before querying the local code directory, so screenshots of already-held shorthand names no longer block import.
- Fixed Portfolio screenshot-import market value calculation: when a visible current/last price is recognized, native market value now uses `last_price × quantity` instead of deriving price from a screenshot market-value column that may be in the account base currency; the default mock quote refresh also no longer overwrites imported market values, and it repairs old mock-overwritten holdings when `current_price` is available in notes.
- Fixed Portfolio draft auto-organization: stale duplicate-code errors are cleared, and market aliases such as `HK` / `港股` merge as the same market so users do not have to clean up automatically resolvable duplicate holdings.
- Fixed Portfolio multi-screenshot imports: draft rows with the same `symbol` now merge automatically, adding quantity and market value and weighting average cost by quantity, so duplicate `0700.HK` rows no longer block confirmation.
- Removed the "Paste screenshot" action from the Portfolio import toolbar; screenshot imports now go through the shared "Add file" picker.
- Refined the Portfolio import toolbar: removed the prominent top-level "Add row" button and moved manual entry into a compact `+` action on the draft table.
- Improved Portfolio local code matching: short Hong Kong numeric codes normalize to the internal `0700.HK` form, so searching or confirming `700` can match/save as `0700.HK`.
- Fixed the Portfolio import draft code input: draft rows now use stable frontend row identities, so editing `symbol` no longer remounts the row or interrupts continuous typing.
- Slimmed the SQLite `security_symbols` schema to only `symbol/name/market/currency/updated_at`; startup migration removes the old provider, exchange, and asset type columns, and the symbol search API only serializes lookup fields.
- Further simplified the Portfolio public default code-directory inventory file: sources only keep `id/count`, and security records only keep `symbol/name/market/currency`, without provider, exchange, or asset type.
- Cleaned the Portfolio public default code-directory inventory data: Traditional Chinese security names from public sources are converted to Simplified Chinese before generating `symbols.json`.
- Simplified the Portfolio public code directory inventory schema: `symbols.json` now keeps only the file-level `updated_at`; individual security records no longer store timestamps, and SQLite imports use the file-level timestamp.
- Changed the Portfolio public code directory default data source to the normalized in-repo inventory file `data/symbol-directory/public/symbols.json`: startup imports that file first, checks `updated_at` for the 24-hour expiry, refreshes public sources asynchronously only after expiry, replaces the inventory file on success, and logs refresh failures without blocking startup.
- Configured the Portfolio public security-code sources through `config/symbol-directory-public.json`: source URLs, parser kinds, cache files, cache directory, and the 24-hour expiry now live in project config; backend refresh reads it on demand and only updates source files after cache expiry.
- Added a local Portfolio security-code directory: the default public provider refreshes SSE, HKEX English/Traditional Chinese, and Nasdaq Trader public directories in the background without an account, caches source files under `data/symbol-directory/public/` with a 24-hour expiry, and folds Traditional/Simplified variants during Chinese-name matching; screenshot recognition and draft confirmation only use local `security_symbols` to fill `symbol`, the import panel keeps "Match codes" for rerunning local matching on an existing draft, and Tushare/authorized sources remain optional extensions.
- Adjusted the Portfolio import draft so opening the tools or clearing the draft no longer inserts an automatic blank row; manual entry now starts from "Add row", and screenshot results append directly into an empty draft.
- Narrowed the frontend surface: the current frontend only exposes Portfolio holdings/import, Memo, and AI settings; Dashboard, Decision Delta, Research, Investment System, and Profile pages plus their frontend API/type/i18n surfaces were removed.
- Cleaned up stale frontend surfaces: removed the unused one-step Portfolio import API wrapper, old import/screenshot preview copy, old AI settings copy, and orphaned request types so the current UI/API surface only exposes wired capabilities.
- Improved the Portfolio import entry point: CSV/TSV/XLSX files and screenshots now share one "Add file" picker, with the frontend routing by file type to CSV/Excel preview or screenshot recognition.
- Simplified the Portfolio import draft table so it only shows the six core fields: `symbol`, `name`, `quantity`, `average_cost`, `currency`, and `market`, hiding source, account, sector, market value, notes, confidence, and issues columns.
- Fixed Portfolio draft confirmation so nonblank draft rows without `symbol` are blocked with localized inline errors in the frontend instead of exposing provider-resolution failures to users.
- Adjusted the Portfolio import tools so manual entry starts from the explicit "Add row" action instead of an automatic placeholder draft row.
- Fixed Portfolio screenshot imports: changed the prompt to a general rule that keeps cash-named ETF/fund/security rows when the holdings table shows quantity, cost/current price, market value, or P/L metrics; when the code cannot be recognized, users must fill `symbol` in the draft table.
- Expanded `AGENTS.md` with development constraints: start new tasks in a fresh worktree, run the smallest relevant checks during iteration, reserve `fmt`, file-length, and clippy checks for final merge/publish readiness or CI coverage, decide and sync project docs/changelogs after each change, start backend/frontend for acceptance testing after finishing, and only commit, push, or open PRs after an explicit user request.
- Added `AGENTS.md` with the default completion flow: verify changes, restart with `make start`, check backend health, and report frontend/backend URLs after each implementation or adjustment.
- Added `cargo clippy --workspace --all-targets -- -D warnings` to CI and exposed the same check locally through `make check-backend-clippy`.
- Fixed Portfolio screenshot import so ETF/fund-like holdings whose names contain "cash" are kept when they have holding metrics such as quantity, cost, and market value, instead of being filtered as pure cash balances.
- Added an 800-line limit for backend Rust source files and split the Portfolio and Decision Delta backend implementations by types, routes, imports, persistence, refresh logic, and related responsibilities.
- Improved the development startup script: `./scripts/dev.sh` now chooses available ports when defaults are occupied and wires the frontend to the selected backend URL.
- Improved AI settings: the Settings page now shows only the fields needed for Mock, OpenAI-compatible, or CLI provider modes, and saving writes the configuration to the local `.env` by default.
- Added a shared AI WebSocket channel and moved Portfolio screenshot recognition to cancelable multi-image tasks; import drafts now support default manual entry, source labels, blank-row filtering, and duplicate-symbol merging.
- Kept the Decision Delta backend/API foundation and performance work: timeline reads are batched, detail snapshots default to the latest 90 entries, refresh reuses quote/FX lookups, and key SQLite indexes were added; the related frontend page is not currently exposed.
- Kept the Decision Delta backend model: buy/add, sell/trim, and watch/skip decisions can create actual legs plus baseline shadow legs and refresh snapshots from current market data; the related frontend page is not currently exposed.
- Polished the Portfolio holdings workflow: CSV/Excel/screenshot inputs now share one editable draft table, confirmed drafts merge-upsert by `symbol`, positions can be edited/deleted, US/HK/CN markets are inferred, summaries use a CNY base view, and market data providers refresh FX with stale fallback.
- Kept the Research Library backend/API foundation for article/person investment-thought distillation, stock snapshot analysis, and portfolio reviews; the related frontend page is not currently exposed.
- Added repository name explanation, vision, ideal goals, and planned capabilities to the README.
- Split bilingual documentation into separate files: Simplified Chinese stays in `.md`, and English uses `.en.md`.
- Made the changelog available in English and Simplified Chinese.
- Added Simplified Chinese alongside English throughout the README.
- Documented changelog ordering: newest entries should be inserted at the top of the current release section.
- Documented the development completion rule: update this changelog after every development change and update the README when setup, capabilities, public interfaces, or workflows change.
- Added engineering style guidance covering readability, maintainability, explainability, Rust design practices, generics, enums, traits, comments, and review expectations.
- Added Codex CLI support as the first CLI AI backend, including device-code login guidance through `codex login --device-auth`.
- Added provider-based AI and market data boundaries, including mock providers, OpenAI-compatible AI, Alpha Vantage-compatible market data, and a reusable CLI AI provider layer.
- Scaffolded the local-first Prudentia workspace with Rust backend, SQLite persistence, React + Vite frontend, Portfolio import, Memo workflows, AI settings, and bilingual UI.
