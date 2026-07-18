# Prudentia

[English](README.en.md)

Prudentia 是一个本地优先的个人投资工作台。当前前端以对话式投资备忘录首页为默认入口，并聚焦 portfolio 持仓导入与展示、memo 管理，以及本地 AI provider 配置。

## 仓库名

`Prudentia` 取意于 prudence 和 practical wisdom：审慎、实践智慧，以及在不确定环境中持续做出更好判断的能力。这个名字强调纪律、复盘和长期主义，而不是短期交易冲动。

## 愿景

Prudentia 希望成为个人投资者的本地优先投资操作系统。它帮助用户把零散研究、投资决策、持仓反馈和自我画像沉淀为可复盘资产，让每一次投资行为都成为长期能力的一部分。

## 理想目标

理想状态下，用户可以在 Prudentia 中形成自己的投资体系，记录每一次决策的假设、风险、催化剂、反证条件和复盘结果。系统会基于这些行为逐渐生成 RPG-like 投资画像，帮助用户理解自己的能力圈、决策纪律、风险偏好和常见偏差。

## 当前能力

- Rust 后端：`axum`、`sqlx`、SQLite、provider-based AI、provider-based market data。
- React + Vite + TypeScript 前端：chat-first 首页、Portfolio、Memos、Settings。
- Chat-first 首页：真实 AI provider 自然对话、可断线续传的持久化运行事件、同一发送控件终止当前任务、附件与研究来源、逐项确认的数据动作，以及持仓/公司看法/使用上下文辅助栏；移动端使用线程与上下文抽屉。
- Portfolio CSV/Excel/截图统一草稿导入、字段映射、本地代码库匹配、确认合并写入、持仓编辑/删除、市值/权重/盈亏和收益率计算、持仓表排序、自动交易调整、CNY 口径汇总、持仓快照收益和组合时间加权收益视角、基准指数对比、ISO 币种金额展示，以及每日 TTL/手动强制行情与 FX 刷新。
- Memo 工作流：创建备忘录，并通过 AI 提取 thesis、风险、催化剂、反证条件和 checklist。
- AI 设置页：支持显式 Mock、OpenAI-compatible 和 CLI provider；默认使用 Codex CLI，支持只在尚未输出正文时切换的有序 fallback 链，并将设置保存到原仓库工作目录的共享 `.env`。
- 支持英文和简体中文 UI，前端通过 `Accept-Language` 控制后端生成文本语言。

## 计划支持的功能

- 更完整的 memo 生命周期：观察、建仓、加仓、减仓、卖出、复盘和归档。
- Portfolio 导入增强：字段映射保存、重复导入处理、账户/市场/行业维度分析。
- 更多行情、AI 和 CLI provider，让外部服务可以通过清晰接口替换。
- 决策复盘提醒，把 review date、decision delta 快照和 thesis 周期变成可执行工作流。
- Research Library、投资体系和画像重新接入前端。
- 投资画像规则扩展，让 XP、属性、徽章和偏差信号更贴近个人投资过程。
- 为券商和交易记录同步预留接口，但保持本地优先和可替换 provider 边界。
- 可导出的投资体系、备忘录和复盘报告，方便长期归档和分享。

## 仓库结构

```text
backend/   Rust API 服务
frontend/  React 应用
docs/      架构、API 和导入说明
examples/  导入模板与示例数据
```

代码风格与可维护性要求见 [docs/engineering-style.md](docs/engineering-style.md)。

重要变更记录见 [CHANGELOG.md](CHANGELOG.md)。每次开发都需要更新 changelog；当启动方式、能力、公共接口或常见工作流变化时同步更新 README。

## 后端

```sh
cp .env.example .env
cargo run -p prudentia-backend
```

后端默认监听 `http://127.0.0.1:8080`，本地数据存储在 `data/prudentia.sqlite`。
使用 `./scripts/dev.sh` 或 `make start` 同时启动前后端时，如果默认端口被占用，脚本会自动选择下一个可用端口并在终端输出实际地址。

常用命令：

```sh
cargo fmt
make check-backend-size
make check-backend-clippy
cargo test -p prudentia-backend
```

## 前端

```sh
npm install --prefix frontend
npm --prefix frontend run dev
```

前端开发服务器默认监听 `http://127.0.0.1:5173`，并将 `/api` 代理到后端。
通过 `./scripts/dev.sh` 启动时，前端会自动连接脚本选出的后端端口。

常用命令：

```sh
npm --prefix frontend run build
```

## 本地配置和数据

后端默认从 Git common dir 所在的原仓库工作目录读取本地 `.env` 和 SQLite 数据，因此从不同 git worktree 启动时会读写同一份原仓库配置和持仓数据。`DATABASE_URL=sqlite://data/prudentia.sqlite` 这类相对 SQLite 路径会相对原仓库工作目录解析，而不是相对当前 worktree。

如果需要自定义本地状态目录，可以设置：

```env
PRUDENTIA_LOCAL_DIR=.prudentia-local
DATABASE_URL=sqlite://data/prudentia.sqlite
```

AI 设置页选择“保存到本地”时，也会写回这份共享 `.env`。

## Provider 默认配置

`AI_PROVIDER=mock` 和 `MARKET_DATA_PROVIDER=mock` 可以让应用在没有外部 API key 的情况下运行；mock 行情只用于离线开发，不会更新真实持仓或指数收益。

`MARKET_DATA_PROVIDER` 支持逗号分隔的 fallback 链，当前可选 `mock`、`yahoo`、`tencent`、`longbridge` 和 `alpha_vantage`。Yahoo 和腾讯行情不需要 API key：

```env
MARKET_DATA_PROVIDER=yahoo,tencent
PRICE_REFRESH_TTL_SECS=86400
```

使用长桥 OpenAPI 行情：

```env
MARKET_DATA_PROVIDER=longbridge,yahoo
LONGBRIDGE_APP_KEY=your_app_key
LONGBRIDGE_APP_SECRET=your_app_secret
LONGBRIDGE_ACCESS_TOKEN=your_access_token
PRICE_REFRESH_TTL_SECS=86400
```

长桥凭证由官方 SDK 从环境变量读取。`LONGBRIDGE_ACCESS_TOKEN` 可能具备账户或交易权限，只应保存在本机 `.env`，不要提交到仓库。

使用 Alpha Vantage-compatible 行情刷新：

```env
MARKET_DATA_PROVIDER=alpha_vantage
ALPHA_VANTAGE_API_KEY=your_key
PRICE_REFRESH_TTL_SECS=86400
```

行情、FX 和基准指数默认按 24 小时 TTL 自动刷新，也可以在持仓页手动触发一次强制刷新。强制刷新会绕过每日 TTL，但仍会经过 provider 级限速、429/频控冷却和 fallback 链；腾讯行情和长桥 provider 会对同一轮持仓/benchmark 报价使用 batch 获取。刷新失败会保留 stale 状态并记录日志，不阻塞启动。

本地证券代码库默认使用免账号 public provider，用于截图或草稿中的名称匹配 `symbol`：

```env
SYMBOL_DIRECTORY_PROVIDER=public
SYMBOL_DIRECTORY_PUBLIC_CONFIG=config/symbol-directory-public.json
SYMBOL_DIRECTORY_REFRESH_INTERVAL_SECS=86400
```

public provider 默认读取项目内的标准化存量文件 `data/symbol-directory/public/symbols.json`，用于启动时导入本地 SQLite 代码库。该文件由 `config/symbol-directory-public.json` 中声明的公开目录生成，包括上交所股票/场内基金、HKEX 英文与繁体中文证券列表，以及 Nasdaq Trader 美股列表。它不需要账号或 token；存量文件中每条证券只保留 `symbol`、`name`、`market`、`currency`，SQLite 的 `security_symbols` 也只保存这四个字段加文件级 `updated_at`，生成前会把繁体中文证券名称统一转换为简体。后端启动时会检查存量文件的 `updated_at`，24 小时内直接复用，过期才异步刷新公开源并覆盖这份存量文件。刷新失败只记录 warning，不阻塞启动或已有本地匹配。匹配只查本地 SQLite 代码库，不在导入时实时请求外部搜索服务。中文名称匹配会做简繁折叠，匹配不到时仍需要在草稿表手动补 `symbol`，或改用后续接入的授权 provider。

可选使用 Tushare 刷新本地证券代码库：

```env
SYMBOL_DIRECTORY_PROVIDER=tushare
TUSHARE_TOKEN=your_token
SYMBOL_DIRECTORY_REFRESH_INTERVAL_SECS=86400
```

使用 OpenAI-compatible chat completions endpoint：

```env
AI_PROVIDER=openai
OPENAI_API_KEY=your_key
OPENAI_BASE_URL=https://api.openai.com/v1
OPENAI_MODEL=gpt-4.1-mini
OPENAI_MODEL_SIMPLE=gpt-4.1-mini
OPENAI_MODEL_STANDARD=gpt-4.1-mini
OPENAI_MODEL_DEEP=gpt-4.1-mini
```

OpenAI-compatible 对话使用真实 SSE token 流。也可以使用通用 CLI provider 接入 Codex CLI 和 ChatGPT/device-code 登录：

```sh
codex login --device-auth
```

然后配置：

```env
AI_PROVIDER=cli
AI_CLI_PROVIDER=codex
AI_CLI_PATH=codex
AI_CLI_MODEL=
AI_CLI_MODEL_SIMPLE=gpt-5.6-luna
AI_CLI_MODEL_STANDARD=gpt-5.6-terra
AI_CLI_MODEL_DEEP=gpt-5.6-sol
AI_CLI_PROFILE=
```

`AI_PROVIDER` 可以配置为有序 fallback 链，例如 `cli,openai`。只有前一个 provider 尚未输出可见正文时才允许切换；不会自动回退到 mock，Settings 保存模型档位时也会保留现有 fallback 链。首页寒暄和普通问题同样由真实 provider 生成，不使用硬编码回复。对话在调用模型前按确定性规则分为轻量、标准和深度：寒暄/短问答优先轻量模型，普通公司与组合讨论使用标准模型，附件、多步骤、财报/经营反证和投资规则修改使用深度模型。前端会同时显示当前阶段、等级、实际模型和选择原因；`AI_CLI_MODEL` 或 `OPENAI_MODEL` 仍可作为兼容的单模型配置，三档变量会覆盖对应等级。

Codex CLI 在 Prudentia 中以临时、隔离且禁用内建工具的模式运行：外部研究由应用自己的 `WebResearchProvider` 和 Capability Registry 执行，CLI 不会读取工作区、调用浏览器或使用 provider 原生 multi-agent。Agent 每轮只返回“调用一个已注册只读 Tool”或“提交最终结构化结果”的决策，真正的 Tool 校验、执行、超时和持久化仍由应用负责。运行区会依次显示研究缓存、资料抓取、来源核验、Skill/Agent 分析、Agent 当前调用的只读 Tool、AI 阅读/分析/组织回复和确认卡提取。公司任务还会展示独立于 Agent 内部轮次的完整研究计划，包括本轮范围、默认模板维度、总进度及每一步状态；计划由后端事件持久化，刷新和断线后不会退回前端猜测。CLI 没有真实 token delta，仍有 Codex agent 启动开销；需要更低首字延迟和真实流式时应使用 OpenAI-compatible provider。轻量、标准和深度任务的可见回复硬上限分别为 90、240 和 600 秒；轻量/标准确认卡提取允许 120 秒，深度任务允许 300 秒。模型正常完成或用户主动终止时会立即结束，不会等待上限耗尽。

应用层 Tool、Skill 和 Agent 由统一 Capability Registry 管理，而不是由 CLI 自由调用。`research_company` 与 `research_community_insights` 是 Rust 原生只读 Tool；`analyze_business_model` 与 `audit_moat` 是可直接运行、也可预加载进 Agent 的版本化 Skill；`analyze_company` 与 `challenge_company_thesis` 是拥有精确 Skill 依赖、只读 Tool 白名单和最多八轮决策预算的 Agent。确定性 planner 先执行基础研究，再把同一份冻结证据交给匹配能力并行处理，最后由回复模型综合。单个 Agent 最多调用四次 Tool，禁止重复同一调用；观察总量限制为 96 KiB，单个能力输出限制为 48 KiB。每次调用使用稳定 `call_id` 和精确版本，开始、Agent 轮次、当前 Tool、完成或失败事件都会持久化；前端可在刷新后恢复，并在 assistant 消息下展示结构化产物和只含轮次、工具、状态、来源数量的限长执行轨迹，不重复保存研究正文。自定义 JSON 清单从共享本地根目录的 `capabilities/` 加载，因此不同 worktree 读取同一份个人能力配置；该目录默认不进入 Git。`GET /api/conversation/capabilities` 会同时列出 Agent 已加载的 Skill 和允许调用的 Tool。公司看法、交易和投资规则写入仍必须逐项确认后由确定性业务代码执行。完整清单契约、扩展边界和规则图复用方式见 [对话能力扩展](docs/capabilities.md)。

内置分析不再共用泛化 Prompt：商业模式、护城河、公司综合判断和反方审计分别有独立 Schema。产物会显式展示证据是否充分、事实/推断/假设、逐条来源与日期、因果链、最强反论、领先指标、证伪条件和对公司判断的影响。证据不足时只返回简短拒绝、关键缺口和最多两个待验证假设，不生成覆盖全部分类的伪分析。标为事实却没有本轮真实来源，或引用本轮未提供 URL 的结果会被后端拒绝；社区观点只能作为待核验线索。

能力形态与工作阶段相互独立：公司深度分析 Agent 属于 `analysis`，反方 Agent 属于 `challenge`。主分析 Agent 已预加载的 Skill 默认不再重复运行，只有用户通过 `@skill_id` 明确点名时才额外生成独立产物；同一运行内重复 URL 的研究来源也只持久化一次。

声明 `rule_graph` 的模型能力还必须显式授权 `rule_graph_input`。规则图固定能力版本与清单内容哈希，并限制节点、模型调用、总时长和执行记录；活动运行 API 会返回未完成能力快照，长事件积压也会完整重放。API 和页面只显示稳定的安全失败类别，不暴露 provider/CLI 内部异常。

默认使用稳定的公共数据源，不需要搜索 API key，也不依赖 AI CLI 的搜索能力：

```env
WEB_RESEARCH_PROVIDER=public_sources
```

“最新财报”、“分析一下 PDD”、“PDD 的护城河是什么”等实质性公司讨论会自动执行三路确定性检索：SEC 最新申报及其真实主文/财报附件、Yahoo Finance 公司经营新闻，以及 TradingView 中带平台 hot 标记、互动数据并且包含经营观点的内容。明确说“只根据/仅使用已有本地上下文”时则跳过检索，直接复用已确认的公司看法。广义公司分析固定先解释商业模式，包括用户、客户、付费方、价值链和资金流；随后分别判断竞争强度、公司相对竞争位置和持续盈利难度，再讨论护城河、所有者经济性、管理层与资本配置、财务韧性和公司质量。SEC inline-XBRL Viewer 链接会还原到真实年报正文，并分别提取业务、竞争、变现、盈利机制、所有者经济性、管理层/激励/资本配置和财务韧性窗口。“近五年财报”、`5 years` 或 `2021—2025` 等多年要求会使用独立缓存，并从 SEC Company Facts XBRL 生成收入、毛利、收入成本、经营利润、净利润、销售营销费用、经营现金流、资本开支、股权激励、稀释加权股数、自由现金流代理值及其每股代理值。自由现金流代理值只表示经营现金流减总资本开支；无法拆分维持性和成长性资本开支时，不会把它称为所有者收益。后端先把本轮整理为只包含公司、结构化意图、标准化期间和三类经营证据查询的 `ResearchPlan`，Tavily 的官方/独立/社区查询各自聚焦且限制长度，搜索 provider 不接收原始对话或仓位数量。回复上下文最多保留每类三条来源：一手来源每条最多 8,000 字符，独立分析和社区观点每条最多 2,500 字符；持久化仍限制为每轮最多九条、每条最多 8,000 字符。完整和部分结果缓存 24 小时；本地只保存压缩后的摘要和来源，不保存原始 Company Facts JSON。

公司线程当前只分析企业经营。首页刷新后打开最近活动线程；确认后的公司看法按版本保存在本地，并在任何绑定该公司的线程右侧“公司看法”中优先展示。传给回复模型的上下文不包含持仓汇总、仓位明细、个人交易历史或投资系统；即使历史消息或来源中出现股价、行情、市值、估值倍数、目标价、涨跌幅、技术面、评级或个人盈亏，也必须忽略。产品市场、行业结构、竞争者和市场份额仍在范围内。在已绑定公司的线程里明确询问另一家公司时，线程绑定保持不变，但本轮研究、来源、公司上下文和轮次摘要切换到明确公司，不复用原公司的摘要、看法或研究缓存。公司简称只有在持仓、线程公司、证券代码和本地证券目录产生唯一高置信候选时才自动解析；受限编辑距离内的唯一英文拼写错误（例如 `netlfix`）也会自动解析，多个同分候选才继续确认。纯字母小写代码还必须紧邻投资、分析、研究或公司问题语境，避免把寒暄词误当证券。多候选或无法识别时，系统会先询问公司全称或证券代码，并且确认前不检索、不回退到线程公司。原请求和候选随确认消息持久化，回复证券代码、“第一个”等候选序号，或在唯一候选确认问句后回复“是的/对/没错”，都会恢复原请求并自动研究；最近一条被终止的 AI 确认问句只有在唯一指向本地证券目录中的公司时才可恢复。新的完整请求会取代旧确认，仍不能唯一确定时才继续询问。识别公司后不会再为默认研究范围阻塞等待：没有额外要求时使用包含商业模式、所有者经济性、竞争位置、护城河、管理层与资本配置、财务韧性、盈利能力和失败机制的综合模板；明确询问单一主题时显示对应专项范围并直接开始研究。回复以公司质量和经营不确定性结束，不生成估值、个股观点、组合影响或买卖结论；自动公司看法补丁也暂不写入 `valuation_expectations`，其中 `thesis` 只表示公司经营逻辑。

护城河分析采用反向审计：高市场份额、好产品、管理/执行、创始人、营销/渠道和暂时技术领先仅是结果或能力，不能直接作为护城河。系统会验证品牌定价权、网络效应、规模经济、转换成本、受保护知识产权和独占资源/牌照等候选机制，并要求说明其如何限制竞争、保护超额利润，以及在取消补贴、竞争者复制、创始人退出和渠道/技术变化时是否仍成立。输出区分短期优势、中期能力和长期结构性护城河，并给出强度、证据与失效条件。

专注护城河分析不会把评分表或风险清单当作分析本身。用户同时提到风险、竞争、反证或“竞争者如何攻破”时，这些内容仍属于护城河审计，不会把回复切换成综合公司报告。每个核心机制必须给出带时间的事实与来源、完整利润保护链、竞争者或外部基准、维持代价、反证与替代解释、信心，以及为什么不是更高或更低一级；没有可观察的 1/3/5 分锚点时不允许打 1—5 分。击穿分析只展开概率×影响最高的三至五条路径，并让每条路径使用同一组显式字段说明攻击者从什么资源出发、最低成本步骤、所需资本/时间/能力、公司可能反制、如何传导至留存/费率/贡献利润/经济利润、领先指标、证伪条件、缺失证据和结论变更点；篇幅不足时减少路径数量，不省略最后一条的字段。待确认的公司看法会按同一字段保存这些结论，而不是重新压成风险摘要；护城河章节最多 3,500 字符，单次公司看法变更总量最多 9,000 字符。

商业模式分析不再只是综合报告中的简短首章。综合公司分析约以 4,000—5,000 字为高密度目标，并把至少 60% 的实质正文留给商业模式；聚焦商业模式时目标为 3,500—4,500 字，核心部分至少占 75%。分析依次拆解产品与用户任务、交易全流程和风险归属、变现方式与利润池、分部单位经济、竞争者及攻击路径、正向复利条件，以及反向失效架构和多元思维；不同产品、地区或履约模式不能用集团平均数混在一起，每部分都区分已核验事实、合理推断和未知。正向部分明确持续复利必须同时成立的条件、增长循环和增量经济性；反向部分从经济利润崩塌或永久损失倒推，即使收入仍增长也可能出现的补贴/廉价资本依赖、渠道/监管/关键人/交易对手风险、瓶颈、反馈反转和二阶连锁反应。系统只选择三至五个具有因果解释力的多元视角，并要求逐项写出观察机制、所需证据和结论影响；最终区分真实价值创造与价值转移，将模式评为脆弱、混合或稳健，并给出最早破坏信号和失效条件。

商业模式、护城河和综合公司分析还会逐项回答六个投资者问题，并先执行可预测性闸门。系统把企业评为“可预测 / 部分可预测 / 不可可靠界定”，列出三至五个关键经营变量，检查其历史稳定性、行业或技术变化、周期、监管、管理层依赖和证据质量。标准化财务基线只是必要条件，不是长期可预测的充分条件；只有闸门通过时才计算五年与十年的经营悲观/经营基准/经营乐观利润或所有者收益区间，闸门不通过时只给定性情景架构、阻断因素和重新量化所需证据。共享审计还覆盖所有者收益及稀释后每股口径、维持性/成长性资本开支、股权稀释、增量投入资本回报、留存收益回报、再投资空间与持续期、great/good/gruesome 经济性分类，以及管理层能力/诚信/坦诚、关键人继任、各方激励、历史资本配置和毁灭性财务风险。回复结尾仍用六行决策矩阵并列事实、乐观、悲观、当前判断与待核验数据；所有情景只描述公司经营，不代表股市熊牛、股价或估值变化。

也可以改用 Tavily：

```env
WEB_RESEARCH_PROVIDER=tavily
TAVILY_API_KEY=your_key
```

只有显式配置 `WEB_RESEARCH_PROVIDER=disabled` 才会关闭自动检索。外部检索不可用不会阻断本地对话，但回复会标记未完成外部核验。对话附件、公司 Markdown 投影与 SQLite 一样保存在原仓库共享本地根目录下；数据库只记录相对路径，因此不同 worktree 会读写同一份本地资料。为限制长期占用，流式 delta 仅在任务活动期间保留，终态后由最终消息替代；研究缓存超过 24 小时会物理清理。投资消息、来源、确认动作、公司/交易/规则版本和组合快照不会自动删除。SQLite 已释放页面留在文件内供后续写入复用，不会在每轮对话后执行高成本 `VACUUM`。

也可以在应用的 Settings 页面编辑 AI 配置。保存后配置会立即生效，并写入原仓库工作目录的共享 `.env`，后端重启或切换 worktree 后继续保留。

## 导入模板

首个支持的 portfolio 导入格式见 [examples/portfolio_import.csv](examples/portfolio_import.csv)。
