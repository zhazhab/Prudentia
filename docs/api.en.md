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

## Investment System

- `GET /api/investment-system/`
- `PATCH /api/investment-system/`
- `POST /api/investment-system/ai/refine`

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
- `GET /api/portfolio/positions`
- `PATCH /api/portfolio/positions/{symbol}`
- `DELETE /api/portfolio/positions/{symbol}`
- `GET /api/portfolio/summary`
- `GET /api/portfolio/performance?period=month|year|since_inception`
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

`PATCH /api/portfolio/positions/{symbol}` supports updating `name`, `quantity`, `average_cost`, `currency`, `account`, `market`, `sector`, `imported_market_value`, and `notes`. `DELETE /api/portfolio/positions/{symbol}` removes closed or incorrect holdings.

`GET /api/portfolio/summary` keeps the legacy native summary fields and also returns:

- `base_currency`: fixed to `CNY`.
- `total_market_value_base` / `total_cost_base` / `total_unrealized_pnl_base`: CNY totals.
- `market_groups`: market + currency groups with native value, CNY value, and weight.
- `fx_rates` / `fx_stale_count`: FX rates and stale state used for the CNY view.

`GET /api/portfolio/performance` returns snapshot-based portfolio performance. `period` accepts `month`, `year`, and `since_inception`; month/year boundaries use Asia/Shanghai calendar periods. The response includes:

- `portfolio`: period start/end CNY value, amount return `end_value_base - start_value_base`, percentage return `end_value_base / start_value_base - 1`, and annualized return `annualized_return_pct`.
- `partial_period`: `true` when no snapshot exists at the period start, so clients can display "since YYYY-MM-DD".
- `series`: portfolio snapshot series, with cumulative return and annualized return when computable.
- `benchmarks`: S&P, Hang Seng, and SSE ETF proxies (`SPY`, `2800.HK`, `510210.SS`) over the same period, with cumulative and annualized returns. These are index proxies, not official index levels; quote failures mark a proxy unavailable/stale without blocking portfolio performance.

Performance v1 is a portfolio value snapshot-change view. It does not adjust for transactions, cash flows, dividends, fees, or splits. Draft confirmation, edit, delete, and daily quote refreshes all write snapshots. The frontend benchmark comparison supports cumulative return, annualized return, and relative excess return; excess return is displayed as portfolio cumulative return minus ETF-proxy cumulative return.

The market data provider refreshes quotes, FX, and benchmark ETFs. The Alpha Vantage provider uses `CURRENCY_EXCHANGE_RATE` for FX; if refresh fails, Prudentia keeps the last successful rate and marks it stale. Backend startup and background jobs check a daily TTL before refreshing, defaulting to 24 hours. `POST /api/portfolio/prices/refresh` remains available as an API/internal capability, but the current frontend does not show a refresh button.

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

`PATCH /api/settings/ai` accepts runtime AI provider settings. Set `persist_to_env` to `true` to write the selected values to `.env`.

```json
{
  "provider": "openai",
  "openai_base_url": "https://api.openai.com/v1",
  "openai_model": "gpt-4.1-mini",
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
  "cli_model": "",
  "cli_profile": "",
  "persist_to_env": true
}
```

Run `codex login --device-auth` before using `cli_provider=codex` on a headless or remote machine. Prudentia does not read or copy Codex's credential cache.
