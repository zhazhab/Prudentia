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

## Portfolio

- `POST /api/portfolio/import/preview`
- `POST /api/portfolio/import/commit`
- `GET /api/portfolio/positions`
- `GET /api/portfolio/summary`
- `POST /api/portfolio/prices/refresh`

Preview request:

```json
{
  "file_name": "positions.csv",
  "content": "symbol,name,quantity,average cost,currency\nAAPL,Apple,2,100,USD"
}
```

Commit request:

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

## Decisions

- `POST /api/decisions/`

```json
{
  "memo_id": "optional-memo-id",
  "symbol": "AAPL",
  "action": "watch",
  "rationale": "Waiting for a better risk/reward entry.",
  "confidence": 0.65,
  "expected_outcome": "Track margin and services mix.",
  "review_date": "2026-09-30"
}
```

## Profile

- `GET /api/profile`

The profile is calculated from memos, decisions, and portfolio state. It is intentionally rule-driven in v1.

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
