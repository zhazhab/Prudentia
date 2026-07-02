# Prudentia API

[English](api.en.md)

Base URL：`http://127.0.0.1:8080`

## Health

- `GET /health`

## Memos

- `GET /api/memos/`
- `POST /api/memos/`
- `GET /api/memos/{id}`
- `PATCH /api/memos/{id}`
- `POST /api/memos/{id}/ai/extract`

`POST /api/memos/` 接收：

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

`kind` 支持 `distillation`、`stock_snapshot` 和 `portfolio_review`。

文章或人物投资思想蒸馏请求：

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

股票快照请求会结合当前持仓、行情、相关 memo 和可选指定 memo：

```json
{
  "symbol": "AAPL",
  "memo_id": "optional-memo-id"
}
```

组合复盘从当前 portfolio positions 生成：

```sh
curl -X POST http://127.0.0.1:8080/api/research/portfolio-review
```

蒸馏、股票快照和组合复盘都会保存为 research record。可将记录里的候选原则/checklist 写入投资体系：

```json
{
  "principles": ["Only underwrite what can be falsified."],
  "checklist_items": ["What would prove the thesis wrong?"]
}
```

## Portfolio

- `POST /api/portfolio/import/preview`
- `POST /api/portfolio/import/draft`
- `POST /api/portfolio/import/image/preview`
- `POST /api/portfolio/import/draft/commit`
- `POST /api/portfolio/import/commit`
- `GET /api/portfolio/positions`
- `PATCH /api/portfolio/positions/{symbol}`
- `DELETE /api/portfolio/positions/{symbol}`
- `GET /api/portfolio/summary`
- `POST /api/portfolio/prices/refresh`

文件预览请求会返回 headers、sample rows、建议 mapping，以及可编辑 `draft_rows`：

```json
{
  "file_name": "positions.csv",
  "content": "symbol,name,quantity,average cost,currency\nAAPL,Apple,2,100,USD"
}
```

用户调整 mapping 后，可以重新生成草稿：

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

导入 `.xlsx` 时，`content` 使用 base64，并将 `content_encoding` 设为 `base64`。

截图识别预览请求：

```json
{
  "file_name": "positions.png",
  "content": "base64-image-content",
  "content_encoding": "base64",
  "mime_type": "image/png"
}
```

截图识别会调用已配置的 Codex CLI provider 识别可见持仓行，并返回同一套 `draft_rows`。文件和截图草稿都需要用户确认后才写入：

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
      "notes": null,
      "confidence": "high",
      "warnings": [],
      "errors": []
    }
  ]
}
```

确认草稿会按 `symbol` 合并更新，不会删除本次草稿中没有出现的旧持仓。任何草稿行存在 `errors` 时都会被拒绝；低置信行只保留 warning，由用户校对后确认。

`PATCH /api/portfolio/positions/{symbol}` 支持更新 `name`、`quantity`、`average_cost`、`currency`、`account`、`market`、`sector`、`imported_market_value` 和 `notes`。`DELETE /api/portfolio/positions/{symbol}` 用于删除清仓或错误持仓。

`GET /api/portfolio/summary` 保留旧的 native 汇总字段，同时返回：

- `base_currency`：固定为 `CNY`。
- `total_market_value_base` / `total_cost_base` / `total_unrealized_pnl_base`：按 CNY 汇总。
- `market_groups`：按 market + currency 分组的 native 市值、CNY 市值和权重。
- `fx_rates` / `fx_stale_count`：用于 CNY 口径的汇率和 stale 状态。

market data provider 会刷新股票报价和 FX。Alpha Vantage provider 使用 `CURRENCY_EXCHANGE_RATE` 获取 FX；刷新失败时会保留最后成功汇率并标记 stale。

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

`quantity` / `notional` / `price` / `currency` / `baseline_type` / `hypothetical_notional` 是可选量化字段。提供后，后端会为支持的动作生成 decision delta legs：

- `buy` / `add`：actual 为实际持仓，baseline 默认为现金。
- `sell` / `trim`：actual 为现金，baseline 默认为继续持有。
- `watch` / `skip`：只有提供 `hypothetical_notional` 时才量化；actual 为保留现金，baseline 默认为假设买入。

## Decision Deltas

- `GET /api/decision-deltas/timeline`
- `GET /api/decision-deltas/timeline?symbol=AAPL&delta=positive&sort=absolute_delta`
- `GET /api/decision-deltas/{decision_id}`
- `GET /api/decision-deltas/{decision_id}?snapshot_limit=30`
- `POST /api/decision-deltas/refresh`
- `PATCH /api/decision-deltas/{decision_id}/review`
- `POST /api/decision-deltas/{decision_id}/adopt`

刷新指定决策，或省略 `decision_ids` 刷新全部可量化决策：

```json
{
  "decision_ids": ["decision-id"]
}
```

时间线返回当前筛选范围内每条决策的最新 snapshot 和汇总。汇总口径是 `sum_of_decision_deltas`，即最新 `actual_value - baseline_value` 之和，不代表完整 portfolio NAV 反事实。

Decision detail 默认返回最近 90 条 snapshots。可用 `snapshot_limit` 调整返回数量；最大值为 365，非法值回退默认值。`latest_snapshot` 始终单独返回最新一条。

保存复盘：

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

采纳复盘候选项到投资体系：

```json
{
  "principles": ["Measure decision deltas before celebrating."],
  "checklist_items": ["What is the no-action baseline?"]
}
```

## Profile

- `GET /api/profile`

画像由 memos、decisions、decision delta reviews 和 portfolio 状态计算。v1 有意采用规则驱动，奖励复盘行为而不是正收益结果。

## Settings

- `GET /api/settings/ai`
- `PATCH /api/settings/ai`

`PATCH /api/settings/ai` 接收运行时 AI provider 配置。将 `persist_to_env` 设为 `true` 会把选中的值写入 `.env`。

```json
{
  "provider": "openai",
  "openai_base_url": "https://api.openai.com/v1",
  "openai_model": "gpt-4.1-mini",
  "openai_api_key": "optional-new-key",
  "persist_to_env": true
}
```

使用 Codex device-code 模式的通用 CLI provider：

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

在 headless 或远程机器上使用 `cli_provider=codex` 前，先运行 `codex login --device-auth`。Prudentia 不读取或复制 Codex credential cache。
