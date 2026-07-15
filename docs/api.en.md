# Prudentia API

[中文](api.md)

Base URL: `http://127.0.0.1:8080`

## Health

- `GET /health`

## Memos

- `GET /api/memos/`
- `POST /api/memos/`
- `GET /api/memos/{id}`
- `PATCH /api/memos/{id}`
- `POST /api/memos/{id}/ai/extract`

`POST /api/memos/` accepts:

```json
{
  "title": "Apple memo",
  "symbol": "AAPL",
  "asset_type": "stock",
  "notes": "Raw research notes",
  "tags": ["quality"]
}
```

## Conversation

The home uses a dedicated durable conversation protocol. Legacy `/api/memo-threads` and `/api/ai/ws` remain only for existing memo and portfolio-screenshot compatibility flows.

- `POST /api/conversation/runs`
- `GET /api/conversation/runs/active`
- `POST /api/conversation/runs/{id}/cancel`
- `POST /api/conversation/runs/{id}/retry`
- `GET /api/conversation/events/ws?after_event_id={cursor}`
- `GET /api/conversation/threads`
- `GET /api/conversation/threads/{id}?message_limit=50&before_message_id={id}`
- `POST /api/conversation/threads/{id}/archive`
- `DELETE /api/conversation/threads/{id}`
- `PATCH /api/conversation/threads/{id}/subject`
- `GET /api/conversation/companies/{symbol}/views`
- `POST /api/conversation/companies/{symbol}/views/{version}/rollback`
- `POST /api/conversation/attachments`
- `PATCH /api/conversation/actions/{id}`
- `POST /api/conversation/actions/{id}/confirm`
- `POST /api/conversation/actions/{id}/reject`

Each `actions[]` item in thread detail includes `assistant_message_id`, allowing confirmation cards to render after the assistant message that proposed the change instead of accumulating at the end of the conversation.

Run creation atomically creates or reuses a thread, persists the user message, and creates the run record. `client_request_id` is the idempotency key; new threads use a temporary `client_thread_id`, while existing threads send `thread_id`:

```json
{
  "client_request_id": "conversation-1",
  "client_thread_id": "client-local-1",
  "content": "Does Tencent's advertising recovery change the thesis?",
  "attachment_ids": ["attachment-id"],
  "locale": "en-US"
}
```

A thread can have at most one `queued`/`running` task, while different threads can run concurrently. Phases are `queued`, `resolving_subject`, `loading_context`, `researching`, `generating`, `extracting_actions`, and `persisting`; terminal states are `completed`, `failed`, `canceled`, and `interrupted`. `researching` appears only when external lookup is needed, and `extracting_actions` is omitted for pure greetings or capability questions with no attachments or research results. Action projection uses its domain-appropriate model tier and a hard timeout based on the original run: 120 seconds after light/standard turns and 300 seconds after deep turns. `ConversationRun` also persists `task_complexity` (`simple` / `standard` / `deep`), the actual `model`, a stable `route_reason`, current `activity`, and `source_count`, allowing refresh to restore the last concrete activity directly. Cancel terminates the real provider process, retry starts from the original user message, and backend startup marks any abandoned active run as `interrupted`.

Company-short-name resolution enters `researching` only for one high-confidence candidate. Multiple candidates or an unrecognized explicit company request use `task_complexity=simple` and `route_reason=subject_clarification`; the real AI receives only the user request and candidate list and asks for the full company name or security code. This turn has `source_count=0`, never reads or falls back to the bound company context, and skips action extraction. The clarification message persists `original_request` and its candidate list in `used_context`; a following unique security code or candidate ordinal resumes that request with the confirmed company, while an ambiguous reply enters clarification again.

The event WebSocket only subscribes to persisted events and does not own task lifetime. Each event has a monotonically increasing `event_id`, `run_id`, `thread_id`, `event_type`, `payload`, and `created_at`. Clients pass the last handled sequence in `after_event_id`; the server replays first and then keeps streaming. Common events are `run.accepted`, `run.classified`, `run.routed`, `run.phase`, `message.delta`, `message.completed`, `source.added`, `action.proposed`, `action.updated`, `run.warning`, and run terminal events. `run.classified` provides the task level and routing reason before model invocation, while `run.routed` records the actual provider/model; both include the updated full `run` snapshot so a concurrent stale active-run fetch cannot overwrite the event. `run.phase.payload.detail.activity` carries application stages such as research-cache lookup, retrieval, and source verification, while `provider_stage` carries provider stages such as reading, analysis, and response writing; these are stable machine codes localized by the client. OpenAI-compatible providers emit real SSE token deltas and switch to response-writing state on the first delta. A CLI provider without token deltas emits phases only and persists the body once at completion instead of faking chunked streaming. `message.delta` rows are durable only while a run is active so refresh and reconnect can recover partial text. Once a run reaches any terminal state, the complete assistant message plus `message.completed` becomes the replay source and its delta rows are compacted. Research-result cache entries are reused for 24 hours and physically deleted during a later lookup after expiry.

Company research normalizes requests such as “last five years,” `5 years`, and `2021-2025` into a 2-10 year annual scope included in the provider cache identity. Public-sources builds annual revenue, gross profit, cost of revenue, operating income, net income, selling-and-marketing expense, and operating-cash-flow series from SEC Company Facts, adding capex, share compensation, diluted weighted-average shares, a free-cash-flow proxy, and its per-share proxy when tags exist. The proxy is only operating cash flow minus total capex, not owner earnings with maintenance investment separated. An isolated annual value that differs from both adjacent facts by at least 50x is removed from that metric and explicitly marked in the summary. Raw Company Facts JSON is never stored. During retrieval, `run.phase.detail.activity` is `research_fetching_financial_history`.

Thread detail returns `thread.subject`, the active run, `latest_run`, paginated messages, confirmation actions, and the current company view. Subjects are restricted to `company`, `investment_system`, `psychology`, and `general`. Subject correction example:

```json
{
  "kind": "company",
  "subject_key": "0700.HK",
  "label": "Tencent"
}
```

Attachment requests accept either a base64 file or a link. Images, PDF, text, Markdown, CSV/TSV, and XLSX are parsed when supported; unsupported files are still stored with an explicit `parse_status` and are never silently treated as read. Originals are content-hash deduplicated under shared `data/workspace`, while responses and the database use relative paths:

```json
{
  "file_name": "earnings.pdf",
  "mime_type": "application/pdf",
  "content_encoding": "base64",
  "content": "..."
}
```

The model can only propose `company_view_patch`, `trade_record`, and `rule_graph_patch` actions. Edit with `{"payload": {...}}`; confirm with `{"expected_version": 3}`. Each action is edited, confirmed, rejected, or failed independently, and target versions plus idempotent execution state prevent duplicate writes. Company views create immutable section-level versions and a Markdown projection. Trades run through deterministic historical-FX, baseline, cost-basis, oversell, and TWR cash-flow checks. A rule patch activates only after DAG, port, configuration, JSON Schema, and adapter-availability validation.

The compatible company-view field `business_quality` has the product meaning “Business Model, Competition, and Profit Quality” and is the first section in confirmation cards, the context panel, and Markdown projections. It covers users/customers/payers, value chain, monetization, competitive intensity, relative competitive position, stakeholder bargaining power, profit pools, pricing power, cost and reinvestment structure, unit economics, capital intensity, and the difficulty of earning durable profits. The remaining compatibility sections are moat, financials, valuation expectations, investment thesis, risks, catalysts, disconfirming evidence, and open questions. Current company-operating analysis does not populate `valuation_expectations` automatically, and `thesis` stores the operating company thesis only.

`business_quality` also retains positive conditions, inverse failure paths, earliest break signals, relevant multidisciplinary lenses, and the fragile/mixed/robust verdict. It now records the predictable/partially predictable/not-predictably-bounded gate, three to five decisive variables, management ability/integrity/candor/owner orientation, succession risk, stakeholder incentive design, and historical capital allocation. A lens cannot be stored as vocabulary alone, and strong management is not automatically a moat.

Company-analysis projections distribute the shared audit and six-row matrix by field. `business_quality` retains the offering, non-substitutability, profit authenticity, maintenance cost, knowability, and management/incentive/capital-allocation verdict. `moat` retains serious competitors, threats, attack paths, and replacement economics. `financials` retains the normalized base, owner earnings and diluted per-share economics, maintenance-versus-growth-capex uncertainty, share compensation and dilution, incremental ROIC, return on retained earnings, reinvestment runway, great/good/gruesome classification, and resilience. Numerical long-range cases are retained only when the knowability gate passes; otherwise it stores qualitative architecture and blockers. Company response context, projection context, and the eventual conversation patch deterministically exclude historical valuation, market, portfolio, and investment-system data. `moat`, `business_quality`, `financials`, and every other section are capped at 3,500, 3,000, 2,000, and 1,200 characters respectively, with 9,000 characters total per patch. When compacting, the projection drops repetition before sources, counterevidence, uncertainty, leading indicators, or verdict-change conditions.

`moat` stores only structural mechanisms that protect durable excess economic returns. Each mechanism carries its causal chain, strength, horizon, evidence, and kill condition. Product quality, market share, management/execution, founder talent, marketing/distribution, and temporary technology leadership are capabilities awaiting proof, not standalone moats. Brand, network effects, scale economies, switching costs, protected IP, and exclusive resources or licenses are also only candidate categories until they survive counterfactual tests for subsidy removal, replicability, customer multi-homing/switching, founder departure, and channel or technology shifts.

Company history is returned newest-version first. A rollback request such as `{"expected_version": 4}` never overwrites history; it copies the target version into a new v5. A current-version mismatch rejects the operation to prevent duplicate rollback or concurrent overwrite.

## Investment System

- `GET /api/investment-system/graph`
- `POST /api/investment-system/graph/evaluate`
- `GET /api/investment-system/legacy`

The active investment system is a versioned DAG. Node kinds are `fixed`, `skill`, and `agent`; the fixed execution kernel supports input, numeric comparison, range, Boolean composition, and output nodes, and evaluation returns both output and node traces. New versions are created and atomically activated only by a confirmed `rule_graph_patch`. `legacy` returns the read-only migration snapshot of the prior natural-language system, which does not participate in execution. The old root and AI-refine paths remain compatibility code, not the home write path.

## Research

- `GET /api/research/records`
- `GET /api/research/records?kind=distillation&symbol=AAPL&q=moat`
- `GET /api/research/records/{id}`
- `POST /api/research/distill`
- `POST /api/research/stock-snapshot`
- `POST /api/research/portfolio-review`
- `POST /api/research/records/{id}/adopt`

`kind` accepts `distillation`, `stock_snapshot`, and `portfolio_review`.

Article or person investment-thought distillation request:

```json
{
  "title": "Munger mental models",
  "source_type": "article",
  "source_title": "The Psychology of Human Misjudgment",
  "source_author": "Charlie Munger",
  "source_content": "Raw article, transcript, notes, or profile material.",
  "symbol": "optional-symbol"
}
```

Stock snapshot requests combine the current holding, quote, related memos, and an optional selected memo:

```json
{
  "symbol": "AAPL",
  "memo_id": "optional-memo-id"
}
```

Portfolio reviews are generated from current portfolio positions:

```sh
curl -X POST http://127.0.0.1:8080/api/research/portfolio-review
```

Distillations, stock snapshots, and portfolio reviews are saved as research records. Candidate principles/checklist items from a record can be adopted into the investment system:

```json
{
  "principles": ["Only underwrite what can be falsified."],
  "checklist_items": ["What would prove the thesis wrong?"]
}
```

## Portfolio

- `POST /api/portfolio/import/preview`
- `POST /api/portfolio/import/draft`
- `GET /api/ai/ws` (WebSocket for screenshot recognition and future AI tasks)
- `POST /api/portfolio/import/draft/commit`
- `POST /api/portfolio/import/commit`
- `GET /api/portfolio/symbols/search?q=Tencent&market=HK&currency=HKD`
- `POST /api/portfolio/symbols/refresh`
- `POST /api/portfolio/symbols/resolve-draft`
- `GET /api/portfolio/positions?period=month|year|since_inception`
- `PATCH /api/portfolio/positions/{symbol}`
- `DELETE /api/portfolio/positions/{symbol}`
- `GET /api/portfolio/summary`
- `GET /api/portfolio/performance?period=month|year|since_inception`
- `GET /api/portfolio/cash-flows?period=month|year|since_inception`
- `POST /api/portfolio/prices/refresh`

File preview returns headers, sample rows, a suggested mapping, and editable `draft_rows`:

```json
{
  "file_name": "positions.csv",
  "content": "symbol,name,quantity,average cost,currency\nAAPL,Apple,2,100,USD"
}
```

After adjusting the mapping, regenerate draft rows:

```json
{
  "file_name": "positions.csv",
  "content": "symbol,name,quantity,average cost,currency\nAAPL,Apple,2,100,USD",
  "mapping": {
    "symbol": "symbol",
    "name": "name",
    "quantity": "quantity",
    "average_cost": "average cost",
    "currency": "currency"
  }
}
```

For `.xlsx` imports, send `content` as base64 and set `content_encoding` to `base64`.

The local security-code directory is used to match draft names to `symbol`. The default `SYMBOL_DIRECTORY_PROVIDER=public` automatically checks the no-account public directory config pointed to by `SYMBOL_DIRECTORY_PUBLIC_CONFIG` in the background at backend startup; the default file is `config/symbol-directory-public.json`. The current provider can also be checked/refreshed through the API:

```sh
curl -X POST http://127.0.0.1:8080/api/portfolio/symbols/refresh
```

Search the local directory:

```sh
curl "http://127.0.0.1:8080/api/portfolio/symbols/search?q=Tencent&market=HK&currency=HKD"
curl "http://127.0.0.1:8080/api/portfolio/symbols/search?q=700&market=HK&currency=HKD"
```

Search and draft matching normalize short Hong Kong numeric codes: for example, `700`, `0700`, or `00700.HK` match the internal `0700.HK` form. When a draft row omits `symbol`, both the matching endpoint and draft confirmation first try to inherit the unique `symbol` from an existing holding with the same name, market, and currency; only then do they query local `security_symbols`.

Run local matching for the current draft:

```json
{
  "rows": [
    {
      "symbol": "",
      "name": "TENCENT HOLDINGS LTD",
      "quantity": "900",
      "average_cost": "489.877",
      "currency": "HKD",
      "account": null,
      "market": "HK",
      "sector": null,
      "imported_market_value": "335646.34",
      "notes": null,
      "confidence": "high",
      "warnings": [],
      "errors": []
    }
  ]
}
```

Matching only uses current holdings and local `security_symbols`; imports do not make live Yahoo or external search requests. For the public provider, source URLs, cache files, normalized inventory file, and expiry are configured in `config/symbol-directory-public.json`; the default inventory file is `data/symbol-directory/public/symbols.json`. Startup imports that file first, and SQLite only stores `symbol/name/market/currency/updated_at`. Public sources are refreshed asynchronously only when `updated_at` is older than 24 hours. Failures are logged and do not block startup.

Screenshot recognition runs over the shared AI WebSocket. Send a text message:

```json
{
  "type": "portfolio_image_import.start",
  "request_id": "import-1",
  "payload": {
    "file_name": "positions.png",
    "content": "base64-image-content",
    "content_encoding": "base64",
    "mime_type": "image/png"
  }
}
```

The server emits `accepted`, `progress`, `completed`, `failed`, and `canceled` messages with the same `request_id`. Screenshot recognition uses the configured Codex CLI provider to extract visible holding rows; the `completed` message contains the same `draft_rows` shape. File and screenshot drafts are written only after explicit user confirmation:

```json
{
  "rows": [
    {
      "symbol": "AAPL",
      "name": "Apple",
      "quantity": "2",
      "average_cost": "100",
      "currency": "USD",
      "market": "US",
      "account": null,
      "sector": "Technology",
      "imported_market_value": "250",
      "last_price": "125",
      "notes": null,
      "confidence": "high",
      "warnings": [],
      "errors": []
    }
  ]
}
```

Draft confirmation first runs the same local matching for rows without `symbol`, then merges duplicate rows by `symbol`: quantity is added and average cost is weighted by quantity. If a row has `last_price`, market value is computed as `last_price × quantity`; otherwise `imported_market_value` is used. Currency or market conflicts are rejected. It then merge-upserts by `symbol` and does not delete existing holdings that are absent from the current draft. Rows with `errors` are rejected; low-confidence rows keep warnings and can be confirmed after user review.

`GET /api/portfolio/positions` accepts an optional `period` query parameter: `month`, `year`, or `since_inception`; omitting it uses the `month` view. Each returned holding includes native `market_value`, FX-converted CNY value `market_value_base`, native `unrealized_pnl`, current return rate `unrealized_pnl_pct`, and position-snapshot-based CNY period P/L `period_profit_loss_base` plus period return `period_return_pct`. The list defaults to `market_value_base` descending; if the holding has no usable starting snapshot in the selected period, the period fields are `null`.

`PATCH /api/portfolio/positions/{symbol}` supports updating `name`, `quantity`, `average_cost`, `currency`, `account`, `market`, `sector`, `imported_market_value`, and `notes`. `DELETE /api/portfolio/positions/{symbol}` removes closed or incorrect holdings.

`GET /api/portfolio/summary` keeps the legacy native summary fields and also returns:

- `base_currency`: fixed to `CNY`.
- `total_market_value_base` / `total_cost_base` / `total_unrealized_pnl_base`: CNY totals.
- `market_groups`: market + currency groups with native value, CNY value, and weight.
- `fx_rates` / `fx_stale_count`: FX rates and stale state used for the CNY view.

Holding returns use the common broker holdings-page convention: native market value is `last_price × quantity`, native unrealized P/L is `(last_price - average_cost) × quantity`, current return rate is `unrealized_pnl / (average_cost × quantity)`, and CNY unrealized P/L sums each holding's native unrealized P/L after FX conversion. Per-holding returns do not treat portfolio-level buy/sell changes as return adjustments.

`GET /api/portfolio/cash-flows` returns system-recorded trade adjustments in the selected period. Import confirmation, draft confirmation, position edits, and position deletes record a `buy` or `sell` adjustment when the CNY portfolio value changes; quote refreshes never create trade adjustments. Trade adjustments only affect portfolio-level return calculations and do not change per-holding cost basis or unrealized P/L.

A confirmed conversation `trade_record` executes through the internal ledger and has no public write route that bypasses its confirmation card. Import/manual holdings are per-security baselines. Post-baseline buys include fees in weighted cost, sells preserve remaining average cost and reject overselling, and buys/sells create TWR inflow/outflow records. Trades before the baseline are history only. Non-CNY trades require execution-date historical FX plus source; corrections use reversal and replacement events.

`GET /api/portfolio/performance` returns portfolio performance. `period` accepts `month`, `year`, and `since_inception`; month/year boundaries use Asia/Shanghai calendar periods. The response includes:

- `portfolio`: period start/end CNY value, amount P/L after net trade adjustments `end_value_base - start_value_base - net_cash_flow_base`, trade-adjusted time-weighted return `return_pct`, unadjusted snapshot return `simple_return_pct`, net trade adjustment `net_cash_flow_base`, annualized return `annualized_return_pct`, and `return_method = "time_weighted"`.
- `partial_period`: `true` when no snapshot exists at the period start, so clients can display "since YYYY-MM-DD".
- `series`: portfolio snapshot series with cumulative net trade adjustment, amount P/L after net trade adjustments, time-weighted cumulative return, unadjusted snapshot return, and annualized return when computable.
- `benchmarks`: S&P ETF proxy `SPY`, Hang Seng ETF proxy `2800.HK`, and the official SSE Composite `000001.SS` over the same period, with cumulative return, annualized return, and the latest quote `source`. Quote failures mark a benchmark unavailable/stale without blocking portfolio performance.

Performance uses portfolio value snapshots and automatic trade adjustments to calculate time-weighted return. Each snapshot interval uses `(ending value - interval net trade adjustment) / starting value - 1`, then compounds the interval returns. Deleting down to an empty portfolio becomes the new starting boundary for later reads, so cleanup followed by reimport is not counted as investment return. Draft confirmation, edit, delete, and daily quote refreshes all write portfolio snapshots and current position snapshots; `GET /api/portfolio/positions?period=...` uses position snapshots to calculate per-position CNY period P/L and return for the same selected period. Benchmark snapshots are written only during the holding price-refresh cycle, so benchmarks and holding prices use the same `price_refresh` run. The frontend benchmark comparison supports cumulative return, annualized return, and relative excess return; excess return is displayed as portfolio time-weighted cumulative return minus benchmark cumulative return.

The market data provider refreshes quotes, FX, and benchmarks. `MARKET_DATA_PROVIDER` accepts a comma-separated fallback chain, such as `yahoo,tencent` or `longbridge,yahoo`; current providers are `mock`, `yahoo`, `tencent`, `longbridge`, and `alpha_vantage`. Yahoo and Tencent quotes do not require API keys; the Tencent provider handles stock quotes and the SSE Composite, and uses Yahoo currency-pair lookup as its FX adapter fallback; the Longbridge provider reads `LONGBRIDGE_APP_KEY`, `LONGBRIDGE_APP_SECRET`, and `LONGBRIDGE_ACCESS_TOKEN`; the Alpha Vantage provider uses `CURRENCY_EXCHANGE_RATE` for FX. `mock` is for offline development only and does not update holding or benchmark returns as available data. Backend startup and background jobs check a daily TTL before refreshing, defaulting to 24 hours. `POST /api/portfolio/prices/refresh` and the holdings-page manual refresh force one refresh, but still go through provider-level minimum request spacing, 429/rate-limit cooldowns, and fallback degradation. Tencent quote and Longbridge batch same-cycle holding/benchmark quote requests; Yahoo/Alpha Vantage remain per-symbol and are serialized by the throttling wrapper. If refresh fails, Prudentia keeps the last successful rate and marks it stale.

## Decisions

- `GET /api/decisions/`
- `POST /api/decisions/`
- `GET /api/decisions/{id}`

```json
{
  "memo_id": "optional-memo-id",
  "symbol": "AAPL",
  "action": "buy",
  "rationale": "Services mix is improving.",
  "confidence": 0.65,
  "expected_outcome": "Track margin and services mix.",
  "review_date": "2026-09-30",
  "decision_date": "2026-07-03",
  "quantity": 10,
  "notional": 1000,
  "price": 100,
  "currency": "CNY",
  "baseline_type": "cash",
  "hypothetical_notional": null
}
```

`quantity` / `notional` / `price` / `currency` / `baseline_type` / `hypothetical_notional` are optional quantification fields. When provided, the backend creates decision delta legs for supported actions:

- `buy` / `add`: actual is the asset exposure; baseline defaults to cash.
- `sell` / `trim`: actual is cash; baseline defaults to continued holding.
- `watch` / `skip`: quantified only when `hypothetical_notional` is provided; actual is cash kept, baseline defaults to hypothetical buy.

## Decision Deltas

- `GET /api/decision-deltas/timeline`
- `GET /api/decision-deltas/timeline?symbol=AAPL&delta=positive&sort=absolute_delta`
- `GET /api/decision-deltas/{decision_id}`
- `GET /api/decision-deltas/{decision_id}?snapshot_limit=30`
- `POST /api/decision-deltas/refresh`
- `PATCH /api/decision-deltas/{decision_id}/review`
- `POST /api/decision-deltas/{decision_id}/adopt`

Refresh selected decisions, or omit `decision_ids` to refresh all quantifiable decisions:

```json
{
  "decision_ids": ["decision-id"]
}
```

The timeline returns the latest snapshot for each decision in the current filter scope. Its summary label is `sum_of_decision_deltas`: the sum of latest `actual_value - baseline_value`, not a full counterfactual portfolio NAV.

Decision detail returns the latest 90 snapshots by default. Use `snapshot_limit` to adjust the returned history; the maximum is 365, and invalid values fall back to the default. `latest_snapshot` is always returned separately.

Save a review:

```json
{
  "notes": "Good process, not just good outcome.",
  "thesis_evidence": ["Services margin expanded."],
  "disconfirming_evidence": ["Hardware cycle softened."],
  "lessons": ["Size slowly when baseline is cash."],
  "candidate_principles": ["Measure decision deltas before celebrating."],
  "candidate_checklist_items": ["What is the no-action baseline?"]
}
```

Adopt review candidates into the investment system:

```json
{
  "principles": ["Measure decision deltas before celebrating."],
  "checklist_items": ["What is the no-action baseline?"]
}
```

## Profile

- `GET /api/profile`

The profile is calculated from memos, decisions, decision delta reviews, and portfolio state. It is intentionally rule-driven in v1 and rewards review process rather than positive return outcomes.

## Settings

- `GET /api/settings/ai`
- `PATCH /api/settings/ai`

`PATCH /api/settings/ai` accepts runtime AI provider settings. Set `persist_to_env` to `true` to write the selected values to the shared `.env` in the original repository working tree behind the Git common dir; `PRUDENTIA_LOCAL_DIR` can override that directory.

```json
{
  "provider": "openai",
  "openai_base_url": "https://api.openai.com/v1",
  "openai_model_simple": "gpt-5.6-luna",
  "openai_model_standard": "gpt-5.6-terra",
  "openai_model_deep": "gpt-5.6-sol",
  "openai_api_key": "optional-new-key",
  "persist_to_env": true
}
```

Generic CLI provider with Codex device-code mode:

```json
{
  "provider": "cli",
  "cli_provider": "codex",
  "cli_path": "codex",
  "cli_model_simple": "gpt-5.6-luna",
  "cli_model_standard": "gpt-5.6-terra",
  "cli_model_deep": "gpt-5.6-sol",
  "cli_profile": "",
  "persist_to_env": true
}
```

Run `codex login --device-auth` before using `cli_provider=codex` on a headless or remote machine. Prudentia does not read or copy Codex's credential cache.
