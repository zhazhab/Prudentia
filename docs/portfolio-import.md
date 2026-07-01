# Portfolio 导入

[English](portfolio-import.en.md)

第一版导入流程：

1. 在前端上传 CSV、TSV 或 XLSX。
2. 后端解析 headers 和 sample rows。
3. 后端建议字段映射。
4. 用户确认字段映射。
5. 后端将归一化后的 positions 写入 SQLite。
6. 后端重新计算 market value、unrealized P/L 和 portfolio weights。

## 截图识别预览

Portfolio 页也支持上传或粘贴 PNG、JPG/JPEG、WebP 截图进行识别。截图识别会调用已配置的 Codex CLI provider，把截图中可见的持仓行提取为可编辑草稿。

截图识别第一版只做 preview：

- 不写入 SQLite。
- 不更新已有持仓。
- 不创建导入历史。
- 不触发 market value、unrealized P/L 或 weight 重算。

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

## 自动更新

自动更新只刷新由行情派生的字段：

- `last_price`
- `market_value`
- `unrealized_pnl`
- `weight`
- `price_updated_at`
- `price_stale`

Position quantity 和 cost basis 仍由导入或手动更新控制。Broker transaction sync 有意不纳入 v1 范围。
