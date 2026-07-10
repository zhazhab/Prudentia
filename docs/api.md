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
- `GET /api/ai/ws`（用于截图识别和未来 AI 任务的 WebSocket）
- `POST /api/portfolio/import/draft/commit`
- `POST /api/portfolio/import/commit`
- `GET /api/portfolio/symbols/search?q=浦发银行&market=CN&currency=CNY`
- `POST /api/portfolio/symbols/refresh`
- `POST /api/portfolio/symbols/resolve-draft`
- `GET /api/portfolio/positions?period=month|year|since_inception`
- `PATCH /api/portfolio/positions/{symbol}`
- `DELETE /api/portfolio/positions/{symbol}`
- `GET /api/portfolio/summary`
- `GET /api/portfolio/performance?period=month|year|since_inception`
- `GET /api/portfolio/cash-flows?period=month|year|since_inception`
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

本地证券代码库用于把草稿中的名称匹配到 `symbol`。默认 `SYMBOL_DIRECTORY_PROVIDER=public` 会在后端启动时自动后台检查 `SYMBOL_DIRECTORY_PUBLIC_CONFIG` 指向的免账号公开目录配置，默认文件为 `config/symbol-directory-public.json`；也可以通过 API 触发当前 provider 检查/刷新：

```sh
curl -X POST http://127.0.0.1:8080/api/portfolio/symbols/refresh
```

查询本地代码库：

```sh
curl "http://127.0.0.1:8080/api/portfolio/symbols/search?q=%E6%B5%A6%E5%8F%91%E9%93%B6%E8%A1%8C&market=CN&currency=CNY"
curl "http://127.0.0.1:8080/api/portfolio/symbols/search?q=700&market=HK&currency=HKD"
```

搜索和草稿匹配会规范化短港股数字代码：例如 `700`、`0700` 或 `00700.HK` 会按 HKEX 代码规则匹配内部 `0700.HK`。草稿行缺少 `symbol` 时，匹配接口和确认导入都会先按名称、市场和币种尝试继承当前已有持仓的唯一 `symbol`；无法唯一确定时才查询本地 `security_symbols`。

对当前草稿执行本地匹配：

```json
{
  "rows": [
    {
      "symbol": "",
      "name": "浦发银行",
      "quantity": "100",
      "average_cost": "10",
      "currency": "CNY",
      "account": null,
      "market": "CN",
      "sector": null,
      "imported_market_value": "1000",
      "notes": null,
      "confidence": "high",
      "warnings": [],
      "errors": []
    }
  ]
}
```

匹配只使用当前已有持仓和本地 `security_symbols`，不会在导入时实时请求 Yahoo 或其他外部搜索服务。public provider 的源 URL、缓存文件、标准化存量文件和过期时间都由 `config/symbol-directory-public.json` 配置；默认存量文件是 `data/symbol-directory/public/symbols.json`。启动时先导入这份文件，SQLite 只保存 `symbol/name/market/currency/updated_at`；只有 `updated_at` 超过 24 小时才异步刷新公开源并替换存量文件，失败只记录错误，不阻塞启动。

截图识别通过统一 AI WebSocket 进行。发送 text message：

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

服务端会用相同 `request_id` 返回 `accepted`、`progress`、`completed`、`failed`、`canceled` 消息。截图识别会调用已配置的 Codex CLI provider 识别可见持仓行；`completed` 消息中包含同一套 `draft_rows`。文件和截图草稿都需要用户确认后才写入：

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

确认草稿会先为缺少 `symbol` 的行执行同一套本地匹配，然后按 `symbol` 归并重复行：数量相加，平均成本按数量加权；如果行里有 `last_price`，市值按 `last_price × quantity` 计算，否则才使用 `imported_market_value`。币种或市场冲突会被拒绝。随后按 `symbol` 合并更新，不会删除本次草稿中没有出现的旧持仓。任何草稿行存在 `errors` 时都会被拒绝；低置信行只保留 warning，由用户校对后确认。

`GET /api/portfolio/positions` 的 `period` 可选，支持 `month`、`year` 和 `since_inception`；省略时等同于 `month` 口径。返回的单只持仓包含本币 `market_value`、按 FX 转换后的 CNY 市值 `market_value_base`、本币 `unrealized_pnl`、当前浮盈亏率 `unrealized_pnl_pct`，以及按持仓快照计算的 CNY 周期收益 `period_profit_loss_base` 和周期收益率 `period_return_pct`。列表默认按 `market_value_base` 降序返回；如果该持仓在所选周期内没有可用起始快照，周期字段为 `null`。

`PATCH /api/portfolio/positions/{symbol}` 支持更新 `name`、`quantity`、`average_cost`、`currency`、`account`、`market`、`sector`、`imported_market_value` 和 `notes`。`DELETE /api/portfolio/positions/{symbol}` 用于删除清仓或错误持仓。

`GET /api/portfolio/summary` 保留旧的 native 汇总字段，同时返回：

- `base_currency`：固定为 `CNY`。
- `total_market_value_base` / `total_cost_base` / `total_unrealized_pnl_base`：按 CNY 汇总。
- `market_groups`：按 market + currency 分组的 native 市值、CNY 市值和权重。
- `fx_rates` / `fx_stale_count`：用于 CNY 口径的汇率和 stale 状态。

持仓收益按券商持仓页常见口径计算：单只持仓本币市值为 `last_price × quantity`，本币浮盈亏为 `(last_price - average_cost) × quantity`，当前浮盈亏率为 `unrealized_pnl / (average_cost × quantity)`；汇总 CNY 浮盈亏为各持仓本币浮盈亏按 FX 转换后的合计。单只持仓收益不把组合层面的买入/卖出变动计入收益率调整。

`GET /api/portfolio/cash-flows` 按所选周期返回系统自动记录的交易调整。导入确认、草稿确认、持仓编辑和持仓删除导致组合 CNY 市值变化时，会按变化方向记录 `buy` 或 `sell` 调整；行情刷新不会产生交易调整。交易调整只影响组合收益率，不改变单只持仓成本或浮盈亏。

`GET /api/portfolio/performance` 返回组合收益表现。`period` 支持 `month`、`year` 和 `since_inception`，边界按 Asia/Shanghai 自然月/自然年。返回包含：

- `portfolio`：周期起止 CNY 市值、扣除净交易调整后的金额收益 `end_value_base - start_value_base - net_cash_flow_base`、交易调整后的时间加权收益率 `return_pct`、未调整交易变动的快照收益率 `simple_return_pct`、净交易调整 `net_cash_flow_base`、年化收益 `annualized_return_pct` 和 `return_method = "time_weighted"`。
- `partial_period`：周期起点没有快照时为 `true`，客户端可显示“自 YYYY-MM-DD 起”。
- `series`：组合快照序列，每个点包含累计净交易调整、扣除净交易调整后的金额收益、时间加权累计收益率、未调整快照收益率和可计算时的年化收益率。
- `benchmarks`：标普 ETF 代理 `SPY`、恒生 ETF 代理 `2800.HK`、官方上证综指 `000001.SS` 的同周期累计收益率和年化收益率；抓取失败时标记 unavailable/stale，不阻塞组合表现。

Performance 使用组合市值快照和自动交易调整计算时间加权收益率。每个快照区间的收益率按 `(期末市值 - 区间净交易调整) / 期初市值 - 1` 计算，再进行连乘。导入确认、编辑、删除和每日行情刷新都会写入组合快照和当前持仓快照；`GET /api/portfolio/positions?period=...` 使用持仓快照计算单只持仓在同一周期内的 CNY 收益和收益率。Benchmark 快照只跟随持仓价格刷新周期写入，确保基准和持仓行情使用同一轮 `price_refresh`。前端基准对比支持累计收益、年化收益和相对基准超额收益三个维度；超额收益按组合时间加权累计收益率减去 benchmark 累计收益率展示。

Market data provider 会刷新股票报价、FX 和 benchmark。`MARKET_DATA_PROVIDER` 支持逗号分隔的 fallback 链，例如 `yahoo,tencent` 或 `longbridge,yahoo`；当前 provider 包含 `mock`、`yahoo`、`tencent`、`longbridge` 和 `alpha_vantage`。Yahoo 和腾讯行情不需要 API key；腾讯 provider 负责股票报价和上证综指，FX 使用 Yahoo 货币对查询作为 adapter fallback；Longbridge provider 使用 `LONGBRIDGE_APP_KEY`、`LONGBRIDGE_APP_SECRET` 和 `LONGBRIDGE_ACCESS_TOKEN`；Alpha Vantage provider 使用 `CURRENCY_EXCHANGE_RATE` 获取 FX。`mock` 只用于离线开发，不会把持仓或 benchmark 收益更新为可用数据。后端启动和后台任务按每日 TTL 检查刷新，默认 24 小时；`POST /api/portfolio/prices/refresh` 和持仓页手动刷新会强制执行一次刷新，但仍受 provider 级最小请求间隔、429/频控冷却和 fallback 降级保护。腾讯行情和长桥会对同一轮持仓/benchmark 报价使用 batch；Yahoo/Alpha Vantage 保持逐只请求并由限速 wrapper 串行化。刷新失败时会保留最后成功汇率并标记 stale。

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
