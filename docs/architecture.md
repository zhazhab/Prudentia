# Prudentia 架构

[English](architecture.en.md)

## 形态

Prudentia 是一个本地优先的 monorepo：

- `backend`：使用 Axum、Tokio、SQLx 和 SQLite 的 Rust API 服务。
- `frontend`：React + Vite + TypeScript 工作台。
- `docs`：产品和实现说明。
- `examples`：导入模板和示例数据。

后端拥有持久化和所有 provider 集成。前端在浏览器中读取文件，将导入内容发送给后端进行 preview/commit，并渲染当前已接通的 portfolio、memo 和 AI settings 工作流。

工程风格记录在 [engineering-style.md](engineering-style.md)。可读性、可维护性和可解释性是架构约束，不是外观偏好。

## 后端模块

- `memo`：thesis notes、risks、catalysts、disconfirming evidence、tags 和 memo AI extraction。
- `research`：本地研究记录、文章/人物思想蒸馏、股票快照、组合复盘，以及候选投资原则/checklist 采纳。
- `investment_system`：个人原则、checklist、能力圈边界和决策规则。
- `portfolio`：导入预览、字段映射、本地证券代码库、提交写入、position 计算、汇总和刷新编排。
- `market_data`：quote/FX provider trait，包含 mock、Yahoo Finance、腾讯行情、长桥 OpenAPI 和 Alpha Vantage-compatible 实现，支持逗号配置 fallback 链、provider 级限速/冷却，以及腾讯/长桥 batch quote。
- `decision`：显式投资决策事件。
- `decision_delta`：为可量化决策创建 actual leg 与 baseline shadow leg，保存每日/手动刷新快照、stale fallback、复盘和候选采纳。
- `profile`：规则驱动的 XP、等级、属性、徽章和偏差信号。
- `ai`：provider trait，包含 mock、OpenAI-compatible 和 CLI-backed 实现。
- `settings`：运行时 AI provider 配置，并支持可选 `.env` 持久化。

## 本地优先默认值

本地 `.env` 和默认 SQLite 数据库默认从 Git common dir 所在的原仓库工作目录读取，用于让不同 git worktree 读写同一份原仓库配置和持仓数据。`PRUDENTIA_LOCAL_DIR` 可以覆盖该目录；相对 SQLite URL 会相对这个本地状态目录解析。

SQLite 是第一版持久化层。v1 不包含登录、多用户授权或券商 API 同步。Portfolio quantity 和 average cost 来自导入或手动更新；自动更新只刷新价格和派生值。

Portfolio Performance 使用组合市值快照模型和系统自动记录的交易调整。导入确认、草稿确认、持仓编辑和持仓删除导致 CNY 组合市值变化时，会在 `portfolio_cash_flows` 写入 `buy` 或 `sell` 调整；每日行情刷新只写快照，不产生交易调整。组合收益率按每个快照区间 `(期末市值 - 区间净交易调整) / 期初市值 - 1` 连乘得到时间加权收益率，同时保留未调整交易变动的快照收益率用于解释。持仓表的周期收益率复用同一个 `本月` / `本年` / `记录起` 周期，按单只持仓快照的 CNY 市值变化计算。标普 ETF 代理、恒生 ETF 代理和官方上证综指快照只跟随持仓价格刷新周期写入，用于同周期收益率对比。持仓浮盈亏按券商持仓页常见口径计算：本币市值为 `last_price × quantity`，本币浮盈亏为 `(last_price - average_cost) × quantity`，当前收益率为 `unrealized_pnl / (average_cost × quantity)`，CNY 汇总再按 FX 转换。

证券代码匹配通过本地 `security_symbols` 目录完成。默认 public provider 会先读取项目内标准化存量文件 `data/symbol-directory/public/symbols.json` 并导入 SQLite；该文件由 `config/symbol-directory-public.json` 中声明的免账号公开目录生成，当前覆盖上交所股票/场内基金、HKEX 英文/繁体中文证券列表和 Nasdaq Trader 美股列表。存量文件中的证券记录只保存 `symbol`、`name`、`market`、`currency`；SQLite `security_symbols` 只多保存文件级 `updated_at`，不再保存 provider、exchange 或 asset type。生成前会将繁体中文证券名称清洗为简体。启动时检查存量文件 `updated_at`，默认 24 小时内复用，过期才在后台异步刷新公开源并覆盖存量文件；刷新失败只记录 warning，不阻塞启动或已有本地匹配。导入确认和截图识别只查本地目录，不对外发起实时模糊搜索，避免 provider 限流或静默猜测。中文匹配会做简繁折叠；授权源例如 Tushare 或券商 OpenAPI 可以作为后续 `SymbolDirectoryProvider` 扩展，用来提升别名和中文名称覆盖率。

Decision Delta v1 不生成无限世界树。每次可量化决策只生成一次 actual/baseline 分叉，之后通过快照记录这个分叉在不同日期的结果；时间线顶部汇总的是当前筛选范围内最新快照的 `actual_value - baseline_value` 之和，不等同于完整组合净值反事实。

## 扩展点

- 通过引入 `BrokerProvider` 模块增加券商同步，并写入归一化 transaction events。
- 扩展 `AiProvider`，加入 memo critique、decision review 和 profile narration 等更丰富 AI 工作流。
- 在现有 `MarketDataProvider` trait 后增加更多 market data provider。
- 在现有证券代码目录后增加更多 `SymbolDirectoryProvider`，例如 Tushare、Choice、Futu OpenAPI 或其他正式授权源。
- 在现有 `AiProvider` trait 后增加更多 AI provider。CLI-backed provider 共享可复用 runner 和 per-tool backend enum；当前 `codex` backend 有意通过 `codex exec` 实现，让 Codex device-code authentication 继续由 Codex CLI 自己拥有。
