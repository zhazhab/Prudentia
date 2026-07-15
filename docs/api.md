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

## Conversation

首页使用独立的持久化对话协议；旧 `/api/memo-threads` 与 `/api/ai/ws` 仅为兼容既有 memo/持仓截图流程保留。

- `POST /api/conversation/runs`
- `GET /api/conversation/runs/active`
- `POST /api/conversation/runs/{id}/cancel`
- `POST /api/conversation/runs/{id}/retry`
- `GET /api/conversation/events/ws?after_event_id={cursor}`
- `GET /api/conversation/threads`
- `GET /api/conversation/threads/{id}?message_limit=50&before_message_id={id}`
- `POST /api/conversation/threads/{id}/archive`
- `DELETE /api/conversation/threads/{id}`
- `PATCH /api/conversation/threads/{id}/subject`
- `GET /api/conversation/companies/{symbol}/views`
- `POST /api/conversation/companies/{symbol}/views/{version}/rollback`
- `POST /api/conversation/attachments`
- `PATCH /api/conversation/actions/{id}`
- `POST /api/conversation/actions/{id}/confirm`
- `POST /api/conversation/actions/{id}/reject`

线程详情中的每个 `actions[]` 项都包含 `assistant_message_id`，用于把确认卡渲染在提出该变更的 assistant 消息之后，而不是统一堆到对话末尾。

创建运行时，服务端在一个事务中创建或复用线程、保存用户消息并创建运行记录。`client_request_id` 是幂等键；新线程使用临时 `client_thread_id`，已有线程传 `thread_id`：

```json
{
  "client_request_id": "conversation-1",
  "client_thread_id": "client-local-1",
  "content": "腾讯广告恢复是否改变了投资逻辑？",
  "attachment_ids": ["attachment-id"],
  "locale": "zh-CN"
}
```

同一个线程最多有一个 `queued`/`running` 任务，不同线程可以并行。阶段为 `queued`、`resolving_subject`、`loading_context`、`researching`、`generating`、`extracting_actions`、`persisting`，终态为 `completed`、`failed`、`canceled` 或 `interrupted`；其中 `researching` 只在需要外部检索时出现，`extracting_actions` 会对无附件、无检索结果的纯寒暄或能力询问省略。动作提取固定使用适合其领域的模型档位，并按原运行难度设置硬超时：轻量/标准为 120 秒，深度为 300 秒。`ConversationRun` 还持久化 `task_complexity`（`simple` / `standard` / `deep`）、实际 `model`、稳定的 `route_reason`、当前 `activity` 和 `source_count`，刷新页面可直接恢复最后一个具体活动。取消会终止真实 provider 进程；重试从原用户消息创建新运行。后端启动时会把遗留活动运行标记为 `interrupted`。

公司简称解析只有唯一高置信候选时才进入 `researching`。多候选或显式公司请求无法识别时，运行使用 `task_complexity=simple`、`route_reason=subject_clarification`，真实 AI 只收到用户原话和候选列表并询问公司全称或证券代码；此轮 `source_count=0`，不会读取或回退到线程绑定公司的上下文，也不会进入动作提取。确认消息的 `used_context` 持久化 `original_request` 和候选列表；下一轮回复唯一证券代码或候选序号时，以已确认公司继续原请求，否则再次进入确认状态。

事件 WebSocket 只订阅持久化事件，不拥有任务生命周期。每个事件包含单调递增的 `event_id`、`run_id`、`thread_id`、`event_type`、`payload` 和 `created_at`。客户端把最后处理的序号传给 `after_event_id`，服务端先重放再持续推送；常见事件为 `run.accepted`、`run.classified`、`run.routed`、`run.phase`、`message.delta`、`message.completed`、`source.added`、`action.proposed`、`action.updated`、`run.warning` 和各类运行终态。`run.classified` 先提供任务等级与分级原因，`run.routed` 提供实际 provider/model，两者都携带更新后的完整 `run` 快照，避免并发活动任务读取覆盖新事件；`run.phase.payload.detail.activity` 提供研究缓存、抓取和来源核验等应用阶段，`provider_stage` 提供 AI 阅读、分析和组织回复等 provider 阶段。这些值是稳定的机器代码，前端负责本地化。OpenAI-compatible provider 发送真实 SSE token delta，并在首个 delta 到达时切换为组织回复；CLI provider 没有 token delta 时只发送阶段，完成后一次持久化正文，不做伪流式分段。`message.delta` 只在运行处于活动状态时持久化，供刷新和断线续传；进入任一终态后，完整 assistant 消息与 `message.completed` 成为重放依据，对应 delta 行会被压缩删除。研究结果缓存复用 24 小时，并在后续检索时物理删除过期行。

公司研究会把“近五年”、`5 years`、`2021—2025` 等要求标准化为 2—10 年的年度范围，并纳入 provider 缓存身份。public-sources 从 SEC Company Facts XBRL 生成逐年收入、毛利、收入成本、经营利润、净利润、销售营销费用、经营现金流，并在标签可用时加入资本开支、股权激励、稀释加权股数、自由现金流代理值及每股代理值。代理值只等于经营现金流减总资本开支，不代表已拆分维持性投入的所有者收益；单年数值若相对前后两年同时出现 50 倍以上孤立数量级异常，会从该指标序列剔除并在摘要中明确标记。原始 JSON 不写入 SQLite。运行期间会发送 `run.phase`，其 `detail.activity` 为 `research_fetching_financial_history`。

线程详情返回 `thread.subject`、活动运行、`latest_run`、分页消息、确认动作和当前公司看法。主题仅允许 `company`、`investment_system`、`psychology`、`general`；修正主题示例：

```json
{
  "kind": "company",
  "subject_key": "0700.HK",
  "label": "腾讯控股"
}
```

附件请求支持 base64 文件或链接。图片、PDF、文本、Markdown、CSV/TSV、XLSX 会按能力解析；不支持的文件仍保存并返回明确的 `parse_status`，不会被当作已读取。原件按内容哈希写入共享 `data/workspace`，响应和数据库只包含相对路径：

```json
{
  "file_name": "earnings.pdf",
  "mime_type": "application/pdf",
  "content_encoding": "base64",
  "content": "..."
}
```

模型只能提出 `company_view_patch`、`trade_record`、`rule_graph_patch` 动作。编辑请求为 `{"payload": {...}}`；确认请求为 `{"expected_version": 3}`。每项动作独立确认、拒绝或失败，并使用目标版本与幂等执行状态防止重复写入。公司看法按章节创建不可变版本和 Markdown 投影；交易由确定性账本代码校验历史汇率、基线、成本、超卖和 TWR 资金流；规则补丁只有通过 DAG、端口、配置、JSON Schema 和 adapter 可用性校验后才会激活。

公司看法兼容字段 `business_quality` 的产品语义是“商业模式、竞争与盈利质量”，也是确认卡、右侧上下文和 Markdown 投影的首章。内容应覆盖用户/客户/付费方、价值链、变现方式、竞争强度、公司相对竞争位置、各方议价权、利润池、定价权、成本与再投资结构、单位经济、资本密集度，以及持续盈利难度；其中竞争强度、相对竞争位置和盈利难度应保留明确评级及其证据。其他兼容章节依次为护城河、财务、估值预期、投资逻辑、风险、催化剂、反证和开放问题；当前公司经营分析不会自动写入 `valuation_expectations`，`thesis` 只保存公司经营逻辑。

`business_quality` 还应保留商业模式的正向成立条件、反向失败路径、最早破坏信号、相关多元视角和脆弱/混合/稳健结论，并记录可预测/部分可预测/不可可靠界定的闸门结果、三至五个关键经营变量、管理层能力/诚信/坦诚/所有者导向、继任风险、各方激励设计和历史资本配置。多元视角不能只记录术语，必须包含观察机制、所需证据及其对判断的影响；管理优秀也不能自动写成护城河。

公司分析投影按字段保存共享审计和六行决策矩阵：`business_quality` 保存产品/服务、不可替代性、利润真实性、优势维护成本、可预测性和管理层/激励/资本配置判断；`moat` 保存重要竞争者、威胁等级、攻击路径及理性攻击者替代经济性；`financials` 保存标准化利润基线、所有者收益及稀释后每股口径、维持性/成长性资本开支不确定性、股权激励与稀释、增量投入资本回报、留存收益回报、再投资空间、great/good/gruesome 分类和财务韧性。只有可预测性闸门通过时才保存五至十年数值经营情景；否则保存定性情景架构、阻断因素和所需证据。`risks` 保存毁灭性压力、缓冲与最早预警。公司回复、投影上下文及最终对话补丁会确定性排除历史估值、行情、组合与投资系统数据。`moat`、`business_quality`、`financials` 和其他单节分别限制为 3,500、3,000、2,000 和 1,200 字符，单次变更总计最多 9,000 字符；超限时优先压缩重复叙述，保留来源、反证、不确定性、领先指标和结论变更条件。

`moat` 只保存能够保护长期超额经济回报的结构机制。每项机制应包含因果链、强度、持续期限、证据和失效条件；好产品、高市场份额、管理/执行、创始人、营销/渠道与暂时技术领先只能作为待验证能力，不能单独写成护城河。品牌、网络效应、规模经济、转换成本、知识产权和独占资源/牌照同样只是候选类别，必须通过去补贴、可复制性、客户多归属/转换、创始人退出和渠道/技术变化等反事实检验。

公司历史按版本倒序返回。回退请求 `{"expected_version": 4}` 不会覆盖旧记录，而是把目标历史内容复制为 v5；当前版本不匹配时拒绝执行，防止重复回退或覆盖并发更新。

## Investment System

- `GET /api/investment-system/graph`
- `POST /api/investment-system/graph/evaluate`
- `GET /api/investment-system/legacy`

活动投资系统是版本化 DAG。节点类型为 `fixed`、`skill`、`agent`，固定执行内核支持输入、数值比较、区间、布尔组合和输出；执行响应包含结果与节点轨迹。新版本只通过已确认的 `rule_graph_patch` 创建并原子激活。`legacy` 返回迁移前自然语言体系的只读快照，不参与规则执行。旧根路径和 AI refine 仅保留兼容代码，不是首页的写入入口。

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

对话确认的 `trade_record` 由内部账本接口执行，不提供绕过确认卡的公开写入路由。导入/手动持仓是证券基线；基线后买入含费用加权成本，卖出保持剩余平均成本并禁止超卖，买/卖分别写入 TWR 流入/流出。成交日早于基线的记录只进入历史。非 CNY 交易必须有成交日历史 FX 及来源；修正通过冲正和替代事件完成。

`GET /api/portfolio/performance` 返回组合收益表现。`period` 支持 `month`、`year` 和 `since_inception`，边界按 Asia/Shanghai 自然月/自然年。返回包含：

- `portfolio`：周期起止 CNY 市值、扣除净交易调整后的金额收益 `end_value_base - start_value_base - net_cash_flow_base`、交易调整后的时间加权收益率 `return_pct`、未调整交易变动的快照收益率 `simple_return_pct`、净交易调整 `net_cash_flow_base`、年化收益 `annualized_return_pct` 和 `return_method = "time_weighted"`。
- `partial_period`：周期起点没有快照时为 `true`，客户端可显示“自 YYYY-MM-DD 起”。
- `series`：组合快照序列，每个点包含累计净交易调整、扣除净交易调整后的金额收益、时间加权累计收益率、未调整快照收益率和可计算时的年化收益率。
- `benchmarks`：标普 ETF 代理 `SPY`、恒生 ETF 代理 `2800.HK`、官方上证综指 `000001.SS` 的同周期累计收益率、年化收益率和最新行情 `source`；抓取失败时标记 unavailable/stale，不阻塞组合表现。

Performance 使用组合市值快照和自动交易调整计算时间加权收益率。每个快照区间的收益率按 `(期末市值 - 区间净交易调整) / 期初市值 - 1` 计算，再进行连乘。删除到空仓会成为后续收益读取的新起点，避免清理后重新导入被误计为收益。导入确认、编辑、删除和每日行情刷新都会写入组合快照和当前持仓快照；`GET /api/portfolio/positions?period=...` 使用持仓快照计算单只持仓在同一周期内的 CNY 收益和收益率。Benchmark 快照只跟随持仓价格刷新周期写入，确保基准和持仓行情使用同一轮 `price_refresh`。前端基准对比支持累计收益、年化收益和相对基准超额收益三个维度；超额收益按组合时间加权累计收益率减去 benchmark 累计收益率展示。

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

`PATCH /api/settings/ai` 接收运行时 AI provider 配置。将 `persist_to_env` 设为 `true` 会把选中的值写入 Git common dir 所在原仓库工作目录的共享 `.env`；`PRUDENTIA_LOCAL_DIR` 可覆盖该目录。

```json
{
  "provider": "openai",
  "openai_base_url": "https://api.openai.com/v1",
  "openai_model_simple": "gpt-5.6-luna",
  "openai_model_standard": "gpt-5.6-terra",
  "openai_model_deep": "gpt-5.6-sol",
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
  "cli_model_simple": "gpt-5.6-luna",
  "cli_model_standard": "gpt-5.6-terra",
  "cli_model_deep": "gpt-5.6-sol",
  "cli_profile": "",
  "persist_to_env": true
}
```

在 headless 或远程机器上使用 `cli_provider=codex` 前，先运行 `codex login --device-auth`。Prudentia 不读取或复制 Codex credential cache。
