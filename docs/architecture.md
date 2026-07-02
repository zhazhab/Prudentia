# Prudentia 架构

[English](architecture.en.md)

## 形态

Prudentia 是一个本地优先的 monorepo：

- `backend`：使用 Axum、Tokio、SQLx 和 SQLite 的 Rust API 服务。
- `frontend`：React + Vite + TypeScript 工作台。
- `docs`：产品和实现说明。
- `examples`：导入模板和示例数据。

后端拥有持久化和所有 provider 集成。前端在浏览器中读取文件，将导入内容发送给后端进行 preview/commit，并渲染 portfolio、memo、profile 工作流。

工程风格记录在 [engineering-style.md](engineering-style.md)。可读性、可维护性和可解释性是架构约束，不是外观偏好。

## 后端模块

- `memo`：thesis notes、risks、catalysts、disconfirming evidence、tags 和 memo AI extraction。
- `research`：本地研究记录、文章/人物思想蒸馏、股票快照、组合复盘，以及候选投资原则/checklist 采纳。
- `investment_system`：个人原则、checklist、能力圈边界和决策规则。
- `portfolio`：导入预览、字段映射、提交写入、position 计算、汇总和刷新编排。
- `market_data`：quote provider trait，包含 mock 和 Alpha Vantage-compatible 实现。
- `decision`：显式投资决策事件。
- `decision_delta`：为可量化决策创建 actual leg 与 baseline shadow leg，保存每日/手动刷新快照、stale fallback、复盘和候选采纳。
- `profile`：规则驱动的 XP、等级、属性、徽章和偏差信号。
- `ai`：provider trait，包含 mock、OpenAI-compatible 和 CLI-backed 实现。
- `settings`：运行时 AI provider 配置，并支持可选 `.env` 持久化。

## 本地优先默认值

SQLite 是第一版持久化层。v1 不包含登录、多用户授权或券商 API 同步。Portfolio quantity 和 average cost 来自导入或手动更新；自动更新只刷新价格和派生值。

Decision Delta v1 不生成无限世界树。每次可量化决策只生成一次 actual/baseline 分叉，之后通过快照记录这个分叉在不同日期的结果；时间线顶部汇总的是当前筛选范围内最新快照的 `actual_value - baseline_value` 之和，不等同于完整组合净值反事实。

## 扩展点

- 通过引入 `BrokerProvider` 模块增加券商同步，并写入归一化 transaction events。
- 扩展 `AiProvider`，加入 memo critique、decision review 和 profile narration 等更丰富 AI 工作流。
- 在现有 `MarketDataProvider` trait 后增加更多 market data provider。
- 在现有 `AiProvider` trait 后增加更多 AI provider。CLI-backed provider 共享可复用 runner 和 per-tool backend enum；当前 `codex` backend 有意通过 `codex exec` 实现，让 Codex device-code authentication 继续由 Codex CLI 自己拥有。
