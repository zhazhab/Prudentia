# Prudentia 架构

[English](architecture.en.md)

## 形态

Prudentia 是一个本地优先的 monorepo：

- `backend`：使用 Axum、Tokio、SQLx 和 SQLite 的 Rust API 服务。
- `frontend`：React + Vite + TypeScript 工作台。
- `docs`：产品和实现说明。
- `examples`：导入模板和示例数据。

后端拥有持久化和所有 provider 集成。前端在浏览器中读取文件，将导入内容发送给后端进行 preview/commit，并渲染当前已接通的 chat-first memo home、portfolio、memo 和 AI settings 工作流。

工程风格记录在 [engineering-style.md](engineering-style.md)。可读性、可维护性和可解释性是架构约束，不是外观偏好。

## 后端模块

- `memo`：thesis notes、risks、catalysts、disconfirming evidence、tags 和 memo AI extraction。
- `conversation`：`ConversationEngine` 深模块，统一主题识别、渐进上下文、外部研究、真实模型调用、任务生命周期、持久化事件和结构化动作提议。
- `memo_thread`：线程与消息的底层持久化、分页、归档和软删除。
- `research`：本地研究记录、文章/人物思想蒸馏、股票快照、组合复盘，以及候选投资原则/checklist 采纳。
- `investment_system`：版本化可执行 DAG、固定规则内核、skill/agent adapter、JSON Schema 校验和执行轨迹；旧自然语言体系只作为迁移资料保留。
- `portfolio`：导入预览、字段映射、本地证券代码库、持仓基线、不可变交易/冲正账本、position 投影、TWR、汇总和刷新编排。
- `market_data`：quote/FX provider trait，包含 mock、Yahoo Finance、腾讯行情、长桥 OpenAPI 和 Alpha Vantage-compatible 实现，支持逗号配置 fallback 链、provider 级限速/冷却，以及腾讯/长桥 batch quote。
- `decision`：显式投资决策事件。
- `decision_delta`：为可量化决策创建 actual leg 与 baseline shadow leg，保存每日/手动刷新快照、stale fallback、复盘和候选采纳。
- `profile`：规则驱动的 XP、等级、属性、徽章和偏差信号。
- `ai`：provider trait，包含 mock、OpenAI-compatible 和 CLI-backed 实现。
- `settings`：运行时 AI provider 配置，并支持写入原仓库工作目录的共享 `.env`。

## 本地优先默认值

本地 `.env`、默认 SQLite、附件原件和公司 Markdown 投影默认从 Git common dir 所在的原仓库工作目录读取，用于让不同 git worktree 读写同一份配置和投资数据。`PRUDENTIA_LOCAL_DIR` 可以覆盖该目录；相对 SQLite URL 会相对这个本地状态目录解析。附件与公司投影位于 `data/workspace`，数据库只保存相对路径。

SQLite 是第一版持久化层。v1 不包含登录、多用户授权或券商 API 同步。Portfolio quantity 和 average cost 来自导入或手动更新；自动更新只刷新价格和派生值。

Portfolio Performance 使用组合市值快照模型和系统自动记录的交易调整。导入确认、草稿确认、持仓编辑和持仓删除导致 CNY 组合市值变化时，会在 `portfolio_cash_flows` 写入 `buy` 或 `sell` 调整；每日行情刷新只写快照，不产生交易调整。组合收益率按每个快照区间 `(期末市值 - 区间净交易调整) / 期初市值 - 1` 连乘得到时间加权收益率，同时保留未调整交易变动的快照收益率用于解释。删除到空仓会成为后续收益读取的新起点，避免数据清理后重新导入被误计为收益。持仓表的周期收益率复用同一个 `本月` / `本年` / `记录起` 周期，按单只持仓快照的 CNY 市值变化计算。标普 ETF 代理、恒生 ETF 代理和官方上证综指快照只跟随持仓价格刷新周期写入，用于同周期收益率对比。持仓浮盈亏按券商持仓页常见口径计算：本币市值为 `last_price × quantity`，本币浮盈亏为 `(last_price - average_cost) × quantity`，当前收益率为 `unrealized_pnl / (average_cost × quantity)`，CNY 汇总再按 FX 转换。

Conversation home 以 thread 为默认交互对象。前端桌面使用线程、主对话、上下文三栏，主对话占主要宽度；移动端把线程和上下文收进左右抽屉。右栏提供当前持仓、公司整体看法和本轮使用上下文。发送与终止复用同一个控件，真正收到正文前不创建空 assistant 气泡；独立运行组件只展示后端持久化的阶段、provider 和耗时。

原始消息与 `conversation_runs` / `conversation_run_events` 是事实来源。`ConversationEngine` 在一个事务中先保存用户输入和运行，再按主题识别、上下文装配、按需研究、自然回复、结构化动作提取和持久化执行。无附件、无检索结果的纯寒暄或能力询问在真实 provider 回复后会跳过无意义的结构化动作提取；投资内容仍执行完整流程。WebSocket 只是带 `event_id` 游标的重放/订阅通道，断开不会取消任务；每线程只允许一个活动运行，不同线程可并行，启动恢复会把遗留任务标为 `interrupted`。不可变轮次摘要、滚动线程摘要、公司看法、持仓和规则图是可从事实记录重建的投影。

所有可见回复都来自配置的真实 provider，包括寒暄和能力介绍。OpenAI-compatible provider 解析真实 SSE token；Codex CLI 持续读取 JSONL 运行事件，没有 token delta 时只更新阶段并在完成后一次展示正文。`AI_PROVIDER` 支持有序 fallback，但只有前一个 provider 尚未输出正文时才能切换，mock 不会被自动选中。外部研究通过 `WebResearchProvider` 接口接入默认的无 key public-sources adapter、可选 Tavily 或 disabled adapter；默认 adapter 并行读取 SEC 最新申报及主文/财报附件、Yahoo Finance 关联新闻/分析和带 hot/互动数据的 TradingView 公司观点。完整与部分结果都按 provider 分区缓存 24 小时，部分失败结果携带证据类别警告。社区来源标记为 `community`，只作为观点与情绪信号。检索不可用时继续使用本地上下文并显式标记未完成外部核验。

研究规划只对已解析为 `company` 的主题生效，provider 边界保留公司名/代码和结构化研究意图，不接收原始对话、仓位或其他私有上下文。public-sources adapter 固定三次检索，Tavily 固定三类查询，因此预算不依赖模型自律。SEC/TradingView 来源只接受各自平台返回的同域 HTTPS URL，TradingView 观点还必须具备 `is_hot` 和非零互动数据；Yahoo/Tavily 返回的 URL 会按解析后的 host 分类，在禁止重定向且锁定公网 DNS 结果的请求中核验。直连被公开站点反爬拒绝时，可通过只读公开网页转换服务复核原 URL 对应的正文。重定向、非成功状态、软 404、私网地址和无页面互动依据的社区来源都不会进入模型上下文；模型同时被要求将网页与附件内容视为不可信证据数据，而不是指令。

模型只生成提议，不直接更新投资数据。公司章节补丁、交易记录和规则图补丁分别生成独立确认卡；用户编辑/确认后，由确定性业务代码执行并通过目标版本和幂等状态防止重复写入。公司确认创建不可变版本与 Markdown 投影；交易确认使用历史汇率和导入基线更新数量、平均成本和 TWR 资金流，修正通过冲正/替代事件完成；规则确认创建并激活通过端口类型、无环性、schema 和 adapter 校验的新 DAG 版本。

证券代码匹配通过本地 `security_symbols` 目录完成。默认 public provider 会先读取项目内标准化存量文件 `data/symbol-directory/public/symbols.json` 并导入 SQLite；该文件由 `config/symbol-directory-public.json` 中声明的免账号公开目录生成，当前覆盖上交所股票/场内基金、HKEX 英文/繁体中文证券列表和 Nasdaq Trader 美股列表。存量文件中的证券记录只保存 `symbol`、`name`、`market`、`currency`；SQLite `security_symbols` 只多保存文件级 `updated_at`，不再保存 provider、exchange 或 asset type。生成前会将繁体中文证券名称清洗为简体。启动时检查存量文件 `updated_at`，默认 24 小时内复用，过期才在后台异步刷新公开源并覆盖存量文件；刷新失败只记录 warning，不阻塞启动或已有本地匹配。导入确认和截图识别只查本地目录，不对外发起实时模糊搜索，避免 provider 限流或静默猜测。中文匹配会做简繁折叠；授权源例如 Tushare 或券商 OpenAPI 可以作为后续 `SymbolDirectoryProvider` 扩展，用来提升别名和中文名称覆盖率。

Decision Delta v1 不生成无限世界树。每次可量化决策只生成一次 actual/baseline 分叉，之后通过快照记录这个分叉在不同日期的结果；时间线顶部汇总的是当前筛选范围内最新快照的 `actual_value - baseline_value` 之和，不等同于完整组合净值反事实。

## 扩展点

- 通过引入 `BrokerProvider` 模块增加券商同步，并写入归一化 transaction events。
- 扩展 `AiProvider`，加入 memo critique、decision review 和 profile narration 等更丰富 AI 工作流。
- 在现有 `MarketDataProvider` trait 后增加更多 market data provider。
- 在现有证券代码目录后增加更多 `SymbolDirectoryProvider`，例如 Tushare、Choice、Futu OpenAPI 或其他正式授权源。
- 在现有 `AiProvider` trait 后增加更多 AI provider。CLI-backed provider 共享可复用 runner 和 per-tool backend enum；当前 `codex` backend 有意通过 `codex exec` 实现，让 Codex device-code authentication 继续由 Codex CLI 自己拥有。
