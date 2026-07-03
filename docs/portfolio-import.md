# Portfolio 导入

[English](portfolio-import.en.md)

当前导入流程：

1. 手动新增行、上传 CSV/TSV/XLSX，或上传/粘贴一张或多张截图。
2. 文件可以追加到当前草稿或替换当前草稿；截图识别任务完成后会追加到同一张草稿表。
3. 用户修正草稿中的字段、删除错误行、合并重复代码，并确认导入。
4. 后端按 `symbol` 合并写入 SQLite，不删除本次未出现的旧持仓。
5. 后端重新计算 market value、unrealized P/L、CNY 口径权重和 portfolio summary。

## 截图识别草稿

Portfolio 页支持上传或粘贴 PNG、JPG/JPEG、WebP 截图进行识别。截图识别通过统一 AI WebSocket 调用已配置的 Codex CLI provider，把截图中可见的持仓行提取为可编辑草稿。多张截图会作为可取消任务追加到同一张草稿表。

截图和文件共用同一套草稿确认流程：

- 不创建导入历史。
- 确认前不写入 SQLite。
- 确认后按 `symbol` 合并更新或新增持仓。
- 重复 `symbol` 行需要先合并或删除，才能确认。
- 不会删除本次草稿中没有出现的旧持仓。

识别结果需要用户自行核对。截图中的隐藏行、合计行或不可见字段不会被推断。

## 必填字段

- `symbol`
- `name`
- `quantity`
- `average_cost`
- `currency`

## 可选字段

- `account`
- `market`
- `sector`
- `imported_market_value`
- `notes`

如果存在 `imported_market_value`，Prudentia 会用 market value 除以 quantity 推导初始 last price。否则会使用 average cost 作为初始 stale price，直到 market data refresh 成功。

## 市场、币种和 CNY 口径

Prudentia 会优先根据 `symbol` 自动推断第一阶段支持的市场：

- `US`：常见美股代码，例如 `AAPL`、`BRK.B`。
- `HK`：港股代码，例如 `0700.HK` 或 5 位以内纯数字代码。
- `CN`：A 股代码，例如 `600519`、`000001`、`.SS`、`.SZ` 后缀。

草稿里的 `market` 和 `currency` 始终可以手动编辑。每个市场按本市场币种展示 native 市值，同时固定使用 `CNY` 作为基础币种计算总持仓额和权重。

Market data provider 会负责刷新股票报价和 FX。Mock provider 使用确定性汇率；Alpha Vantage provider 使用 `CURRENCY_EXCHANGE_RATE`。如果 FX 刷新失败，系统会保留最后成功汇率并标记为 stale。

## 自动更新

自动更新只刷新由行情派生的字段：

- `last_price`
- `market_value`
- `unrealized_pnl`
- `weight`
- `price_updated_at`
- `price_stale`

Position quantity 和 cost basis 仍由导入或手动更新控制。Broker transaction sync 有意不纳入 v1 范围。
