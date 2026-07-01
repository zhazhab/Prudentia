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
- `POST /api/portfolio/import/image/preview`
- `POST /api/portfolio/import/commit`
- `GET /api/portfolio/positions`
- `GET /api/portfolio/summary`
- `POST /api/portfolio/prices/refresh`

预览请求：

```json
{
  "file_name": "positions.csv",
  "content": "symbol,name,quantity,average cost,currency\nAAPL,Apple,2,100,USD"
}
```

提交请求：

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

截图识别会调用已配置的 Codex CLI provider 识别可见持仓行，只返回可编辑草稿和提示，不写入 `portfolio_positions`，也不会触发权重或行情重算。

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

画像由 memos、decisions 和 portfolio 状态计算。v1 有意采用规则驱动。

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
