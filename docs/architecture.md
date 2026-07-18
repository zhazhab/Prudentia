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

本地 `.env`、默认 SQLite、附件原件、公司 Markdown 投影和自定义 Capability 清单默认从 Git common dir 所在的原仓库工作目录读取，用于让不同 git worktree 读写同一份配置、投资数据和个人分析方法。`PRUDENTIA_LOCAL_DIR` 可以覆盖该目录；相对 SQLite URL 会相对这个本地状态目录解析。附件与公司投影位于 `data/workspace`，能力清单位于 `capabilities/`，数据库只保存附件和投影的相对路径。

SQLite 是第一版持久化层。v1 不包含登录、多用户授权或券商 API 同步。Portfolio quantity 和 average cost 来自导入或手动更新；自动更新只刷新价格和派生值。消息、来源、确认动作、公司版本、交易、规则图和组合快照作为投资事实长期保留。运行中的 `message.delta` 只为断线恢复保留，在终态完整消息落库后删除；研究缓存超过 24 小时后由后续检索清理。创建对话运行使用 `BEGIN IMMEDIATE` 在读取线程状态前取得写保留锁，避免证券目录刷新等后台写入与延迟事务发生不可等待的读写升级冲突。SQLite 释放的页留给后续写入复用，不在每轮对话执行 `VACUUM`，避免无收益的全库重写和磁盘抖动。

Portfolio Performance 使用组合市值快照模型和系统自动记录的交易调整。导入确认、草稿确认、持仓编辑和持仓删除导致 CNY 组合市值变化时，会在 `portfolio_cash_flows` 写入 `buy` 或 `sell` 调整；每日行情刷新只写快照，不产生交易调整。组合收益率按每个快照区间 `(期末市值 - 区间净交易调整) / 期初市值 - 1` 连乘得到时间加权收益率，同时保留未调整交易变动的快照收益率用于解释。删除到空仓会成为后续收益读取的新起点，避免数据清理后重新导入被误计为收益。持仓表的周期收益率复用同一个 `本月` / `本年` / `记录起` 周期，按单只持仓快照的 CNY 市值变化计算。标普 ETF 代理、恒生 ETF 代理和官方上证综指快照只跟随持仓价格刷新周期写入，用于同周期收益率对比。持仓浮盈亏按券商持仓页常见口径计算：本币市值为 `last_price × quantity`，本币浮盈亏为 `(last_price - average_cost) × quantity`，当前收益率为 `unrealized_pnl / (average_cost × quantity)`，CNY 汇总再按 FX 转换。

Conversation home 以 thread 为默认交互对象。前端桌面使用线程、主对话、上下文三栏，主对话占主要宽度；移动端把线程和上下文收进左右抽屉。右栏提供当前持仓、公司整体看法和本轮使用上下文。发送与终止复用同一个控件，真正收到正文前不创建空 assistant 气泡；独立运行组件只展示后端持久化的具体阶段、活动、任务等级、实际模型、选择原因、provider、来源数量和耗时，并逐项显示并行 Skill/Agent。公司任务另外展示确定性的完整执行计划，而不是把 Agent 的 `(当前轮/总轮数)` 当作研究计划：范围、默认模板维度和每个步骤都由 `run.plan.created` / `run.plan.step` 驱动。活动运行的 REST 快照包含重放事件后还原的 `execution_plan` 和未完成 Capability，WebSocket 会分批追完持久化游标，前端还能合并先于 REST 到达的事件。完成后的结构化 Capability 产物作为 assistant 消息旁的独立组件展示，刷新不依赖前端重新猜测活动或重新执行分析。

原始消息与 `conversation_runs` / `conversation_run_events` 是事实来源。`ConversationEngine` 在一个事务中先保存用户输入和运行，再按主题识别、确定性任务分级、上下文装配、按需研究、分级模型调用、结构化动作提取和持久化执行；回复生成与动作投影/持久化分为独立编排函数。线程持久化主题与本轮有效主题彼此独立：在已绑定公司线程中显式提到另一家公司时，只为本轮切换公司，不修改线程绑定；上下文会排除原绑定公司的线程摘要、历史轮次、公司看法和研究缓存，轮次摘要也按本轮有效公司读写。独立 `subject_resolution` 模块把持仓、线程公司、代码和本地证券目录收敛为候选，只有唯一高置信结果才生成公司主题；对 5—32 个 ASCII 字母的显式公司提示，还允许在本地目录首字母候选中做受限 Damerau-Levenshtein 匹配，短名称最多一处、十字符以上最多两处编辑，并且只有唯一结果才自动绑定。纯字母小写代码只有紧邻投资、持有、分析、研究、介绍或公司问题标记时才构成强引用，避免把普通英文词或寒暄误识别为证券。多候选、模糊匹配并列或显式公司请求无匹配时返回确认状态，不得回退到线程公司。确认状态使用轻量真实 AI 和只含用户原话/候选的最小上下文，不检索、不投影动作，也不更新绑定线程摘要；原请求和候选随 assistant 消息持久化，唯一代码或候选序号会恢复原请求，无法唯一选择则继续确认。公司身份确定后，`execution_plan` 按消息意图选择专项范围或默认综合模板并立即开始，不再增加阻塞式范围确认；范围和模板仅用于向用户解释后端将执行什么，不替代底层 `ResearchPlan` 或 Capability 编排。结构化动作提取是独立任务：普通公司/交易提取固定使用标准档，避免跟随深度正文使用更慢模型；轻量/标准运行的提取窗口为 120 秒，深度运行为 300 秒。只有解析为投资系统主题的任务使用深度档并允许产生规则图，其他主题的规则图提议会被拒绝。公司回复和投影上下文排除历史估值、交易和投资系统；公司补丁在准备和执行阶段也会确定性移除估值变更。CLI 与 OpenAI-compatible 回复都由 `serde_json` 依次寻找第一个匹配目标类型的完整对象，合法对象后的说明不会破坏提取。WebSocket 仍只是带 `event_id` 游标的重放/订阅通道。

对话入口模块只暴露运行、线程和确认动作 API，主题解析、工具编排、生成和动作提取由独立 runtime 深模块执行。主题与路由确定后冻结 `TurnContext`，其中包含确定性的有界 `ToolPlan`；只读工具结束后再冻结包含实际模型输入和已持久化来源的 `StepContext`，后续阶段不再重新采样可变线程状态。所有事件由强类型 `ConversationEvent` 生成并保持稳定 wire format。`TurnTask` 将一个取消信号传入工具、正文生成和动作提取，先等待协作退出，超时才强制中止。完成、失败、取消和中断通过数据库条件更新竞争同一个终态，只有第一个写入者生效。

`conversation::capabilities` 是统一能力边界，外部只暴露确定性 planning、执行和已注册规则节点 adapter。Capability Registry 按稳定 `id + version` 注册 Rust `native` 工具、声明式 `skill` 和声明式 `agent`，统一校验输入/输出 JSON Schema、主题范围、最小上下文权限、模型档位、只读副作用、确认策略、超时、surface、存储策略和首个用户可见 activity。Skill 是可直接运行或被 Agent 预加载的版本化分析方法；Agent 是有独立 instructions、精确 Skill 依赖、只读 Tool 白名单和最多八轮预算的模型会话。运行先执行 Research stage，再冻结 `ConversationContext`，随后让 Analysis/Challenge stage 中匹配的 Skill/Agent 基于同一快照并行运行；反方 Agent 不读取其他能力输出。Agent 每轮只能选择一个精确版本白名单 Tool 或提交最终 Schema 输出，所有 Tool 仍经确定性 Registry 校验与执行，最多四次且禁止等价重复调用。每次外层调用保留稳定 `call_id`，统一处理进度、取消、失败隔离和产物汇总；内部轮次、工具和来源数量形成有界 `agent_trace`。一个 turn 最多三个模型 Capability，单个输出最多 48 KiB，Agent 观察最多 96 KiB。研究失败进入外部核验降级，模型 Capability 失败成为独立失败产物而不污染研究状态；原始 provider/CLI 错误只进服务端日志，持久化事件和产物使用稳定公开错误类别。

内置 `research_company` 和 `research_community_insights` 继续共享结构化 `ResearchPlan` 与来源验证，但使用独立缓存作用域；它们对 Agent 只暴露一个有界 `focus`，真正检索计划仍由 Rust planner 生成。纯社区问题只执行社区类别，同时要求商业模式、财务、护城河或综合判断时仍使用完整公司研究计划。内置 `analyze_business_model`、`audit_moat` 是可复用方法；`analyze_company` 与 `challenge_company_thesis` 会加载这些方法，并按证据缺口自主选择公司研究或社区研究。自定义 Skill/Agent 从 Git common dir 对应共享本地根目录的 `capabilities/**/*.json` 在启动时加载；未知 Schema 关键字、不可解析依赖、写 Tool、符号链接和无界配置会被拒绝，单个坏文件不会压制相邻有效文件。应用不暴露任意 `load_skill(path)`、`spawn_agent(prompt)`、Shell、浏览器或插件透传；CLI provider 只返回结构化决策，不拥有工具生命周期。完整契约见 [对话能力扩展](capabilities.md)。

模型 Capability 的 Prompt 由 Schema 感知的运行时协议、清单方法 instructions 和输出 Schema 三层组成。四个内置分析能力分别拥有领域类别，而不是共享泛化模板；统一 finding 契约保存证据充分性、事实类型、来源与日期、因果链、反论、未知项、领先指标、证伪和决策影响。模型只能引用冻结上下文或 Agent Tool 观察中的精确 URL；运行时递归校验 `source_urls`，并拒绝无引用的 `fact`。社区来源保持待核验信号。研究失败且证据不足时，Agent 早停为简短拒绝、关键缺口和最多两个待验证假设，不用模型记忆补齐，也不为了完整感遍历所有类别。历史字符串证据只在前端兼容层转换，不改变数据库原文。

声明了 `rule_graph` surface 且显式授权 `rule_graph_input` 的 Skill/Agent 会以 `id@version` 注册进 `RuleNodeAdapterRegistry`。规则图激活和执行使用同一份注册表，校验节点/adapter 类型、Capability arguments、DAG 及节点 Schema，并把 `manifest_hash` 固定到激活版本；同一版本清单被原地修改时执行会拒绝。每张图限制 64 节点、256 边、8 个模型节点和 512 KiB，整图 600 秒超时并只保留最近 500 次执行；输入只存一次，节点 trace 通过引用避免重复上下文。模型不能直接发起数据写入；公司看法、交易和规则图仍由动作投影生成确认卡，确认后才进入确定性领域代码。

`conversation::research::cache` 统一拥有普通公司与社区专项研究的缓存读取、哈希和过期清理，provider 与工具 adapter 不直接操作缓存表。

社区专项计划保留已确认公司与结构化问题意图，但不继承综合公司分析默认的五年财务范围，避免社区查询混入无关年报关键词。

所有可见回复都来自配置的真实 provider，包括寒暄和能力介绍。OpenAI-compatible provider 解析真实 SSE token；Codex CLI 使用临时、忽略用户配置/规则且禁用 workspace、shell、浏览器、插件和多 agent 工具的执行模式。广义公司分析先映射用户/客户/付费方、价值链和资金流，再判断竞争强度、相对竞争位置和持续盈利难度，之后分析护城河、默认五年财务、经营风险和公司质量。公司主题的回复上下文排除持仓汇总、仓位明细、交易历史和规则图；股价、行情、市值、估值倍数、目标价、涨跌幅、技术图表、评级、个人盈亏和买卖含义即使出现在历史或来源中也不得使用。产品市场、行业结构、竞争者和市场份额仍属于公司经营证据。轻量、标准和深度可见回复分别设置 90、240 和 600 秒硬上限；轻量/标准动作提取为 120 秒，深度动作提取为 300 秒。外部研究通过 `WebResearchProvider` 接入默认 public-sources、可选 Tavily 或 disabled adapter；默认 adapter 读取 SEC 公司申报、Yahoo Finance 公司经营新闻和带 hot/互动数据的 TradingView 经营观点，并过滤只谈股票估值、价格或交易技术的结果。回复上下文按来源等级最多保留每类三条，一手材料每条最多 8,000 字符，独立分析和社区观点每条最多 2,500 字符；持久化仍限制为每轮九条、每条 8,000 字符。研究执行与持久化进度写入独立调度，取消父运行会终止研究任务，避免连接池等待和遗留检索。广义/商业模式分析和明确的多年财务请求会读取已核验的 SEC Company Facts XBRL。完整与部分结果按 provider 缓存 24 小时；检索不可用时显式标记未完成外部核验。

护城河问题属于深度任务并使用独立回复结构。风险、竞争、反证和“如何攻破”等措辞属于该审计的内部维度，只有同时要求商业模式、财报或估值等独立主题时才进入综合公司结构。系统先排除市场份额、产品、管理、执行、创始人、营销/渠道和短期技术领先等“能力即护城河”的推断，再沿“结构机制 → 客户或成本行为 → 竞争者受限 → 超额利润留存”验证候选机制。广义公司分析内也执行同一审计。专注护城河分析要求每个机制形成带事实来源、竞争者对照、维护成本、反证、信心和相邻结论条件的判断卡；没有可观察锚点时不得使用 1—5 分。击穿路径按概率×影响排序，只展开最重要的三至五项；每项重复相同字段并保存攻击步骤、资源要求、公司反制、经济利润传导、领先指标、证伪条件、缺失证据和结论变更点，篇幅不足时减少路径而不是截断末项。最终输出和 `company_view_patch.moat` 必须保留这些依据以及强度、短期/中期/结构性期限和失效条件；投影允许 `moat` 使用最多 3,500 字符，所有公司看法变更合计最多 9,000 字符，并优先压缩重复叙述。用户提供材料只用于提炼方法，不把其中未经核验的个股结论注入公司看法。

商业模式问题同样属于深度任务，并在广义公司分析之外拥有专用结构。综合分析把至少 60% 的实质正文留给商业模式，聚焦请求至少保留 75%；结构依次覆盖产品/用户任务、交易全流程与风险归属、变现/利润池、分部单位经济、竞争/攻击者经济、正向复利条件，以及反向失效和多元思维。不同产品、地区或履约模式必须分别分析，集团增长不能替代分部单位经济证据；每节区分事实、推断与未知。随后从微观经济/产业组织、会计与公司金融、心理与激励、系统思维、博弈论、技术运营、组织监管及外部基准中选择三至五个有因果相关性的视角。每个视角必须形成“模型 → 观察机制 → 所需证据 → 结论影响”，避免模型名称堆砌。综合结论区分价值创造与价值转移，输出脆弱/混合/稳健评级、最早破坏信号、反证和失效条件，并由动作投影压缩进 `business_quality`。

广义公司、商业模式和护城河回复结构由 `ai::prompt::company_analysis` 集中管理，并统一追加共享投资者问答和巴菲特/芒格式经营审计。长期情景前必须先通过可预测性闸门：列出三至五个关键经营变量，检查历史稳定性、行业/技术变化、周期、监管、管理层依赖和证据质量，再评为可预测、部分可预测或不可可靠界定。标准化利润基线只是预测起点；只有变量可界定时才计算五年和十年的经营悲观/基准/乐观利润或所有者收益区间，否则保存定性情景架构、阻断因素和重新量化所需证据。共享审计还单列所有者收益及稀释后每股口径、维持性/成长性资本开支、股权稀释、增量投入资本回报、留存收益回报、再投资空间与 great/good/gruesome 分类，管理层能力/诚信/坦诚、继任、各利益相关方激励和历史资本配置，以及偿债、流动性、或有负债与永久毁损路径。回复仍以六行决策矩阵并列事实、乐观、悲观、当前判断和缺失证据；公司经营情景不得混入股市熊牛、股价或估值倍数。

研究规划只对已解析为 `company` 的主题生效。独立 planner 将原始消息收敛为 `ResearchPlan`，其中只有公司名/代码、结构化意图、标准化年度范围和带证据类别的公司经营查询；provider 不接收原始对话、仓位或其他私有上下文，也不生成估值或行情查询。Tavily 的三条查询共享紧凑意图并分别聚焦官方、独立和社区证据；每条查询受测试约束，每个来源摘要在缓存和入库前最多保留 8,000 字符。默认 public-sources 不依赖关键词搜索：SEC Company Facts 以结构化单位生成资本开支、股权激励、稀释股数、自由现金流代理值及每股代理值，20-F/10-K 摘录在固定 4,500 字符内覆盖所有者经济性、管理层/激励/资本配置和财务韧性；本地仍不保存原始 XBRL JSON。

模型只生成提议，不直接更新投资数据。公司章节补丁、交易记录和规则图补丁分别生成独立确认卡；用户编辑/确认后，由确定性业务代码执行并通过目标版本和幂等状态防止重复写入。公司确认创建不可变版本与 Markdown 投影；交易确认使用历史汇率和导入基线更新数量、平均成本和 TWR 资金流，修正通过冲正/替代事件完成；规则确认创建并激活通过节点输入/输出 Schema、无环性和精确版本 adapter 校验的新 DAG 版本。

证券代码匹配通过本地 `security_symbols` 目录完成。默认 public provider 会先读取项目内标准化存量文件 `data/symbol-directory/public/symbols.json` 并导入 SQLite；该文件由 `config/symbol-directory-public.json` 中声明的免账号公开目录生成，当前覆盖上交所股票/场内基金、HKEX 英文/繁体中文证券列表和 Nasdaq Trader 美股列表。存量文件中的证券记录只保存 `symbol`、`name`、`market`、`currency`；SQLite `security_symbols` 只多保存文件级 `updated_at`，不再保存 provider、exchange 或 asset type。生成前会将繁体中文证券名称清洗为简体。启动时检查存量文件 `updated_at`，默认 24 小时内复用，过期才在后台异步刷新公开源并覆盖存量文件；刷新失败只记录 warning，不阻塞启动或已有本地匹配。导入确认和截图识别只查本地目录，不对外发起实时模糊搜索，避免 provider 限流或静默猜测。中文匹配会做简繁折叠；授权源例如 Tushare 或券商 OpenAPI 可以作为后续 `SymbolDirectoryProvider` 扩展，用来提升别名和中文名称覆盖率。

Decision Delta v1 不生成无限世界树。每次可量化决策只生成一次 actual/baseline 分叉，之后通过快照记录这个分叉在不同日期的结果；时间线顶部汇总的是当前筛选范围内最新快照的 `actual_value - baseline_value` 之和，不等同于完整组合净值反事实。

## 对话安全边界

普通标点和“更新公司看法”等指令不能单独构成公司提示；公司研究只接受明确公司分析结构或唯一高置信候选，无法唯一识别时必须先请用户确认。待确认状态消费候选序号、证券代码、短公司名或唯一候选后的明确肯定；如果确定性确认最初没有候选，但最近一条 AI 确认问句唯一指向本地证券目录中的公司，即使该回复被终止也可以恢复原始请求。多个候选不能通过笼统肯定选择，新的完整请求会取代旧确认。明确要求只使用已有本地上下文时，planner 不生成外部检索，短问题固定使用标准模型。Codex CLI 的动作投影使用原生 JSON Schema 强制 `summary` / `actions` 外层契约，并在动作进入确定性业务代码前再次解码和校验载荷。格式异常只生成持久化运行告警，不会直接修改公司看法、交易或投资规则。

## 扩展点

- 通过引入 `BrokerProvider` 模块增加券商同步，并写入归一化 transaction events。
- 扩展 `AiProvider`，加入 memo critique、decision review 和 profile narration 等更丰富 AI 工作流。
- 在现有 `MarketDataProvider` trait 后增加更多 market data provider。
- 在现有证券代码目录后增加更多 `SymbolDirectoryProvider`，例如 Tushare、Choice、Futu OpenAPI 或其他正式授权源。
- 在现有 `AiProvider` trait 后增加更多 AI provider。CLI-backed provider 共享可复用 runner 和 per-tool backend enum；当前 `codex` backend 有意通过 `codex exec` 实现，让 Codex device-code authentication 继续由 Codex CLI 自己拥有。
