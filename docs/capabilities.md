# 对话能力扩展

Prudentia 把对话中的可复用工作单元统一称为 **Capability**。Capability Registry 同时管理三类能力：

- `native`：由 Rust 实现的确定性应用工具，例如公司资料检索和社区观点检索。它可以访问明确授权的基础设施，但模型不能自行创建或调用任意系统工具。
- `skill`：版本化的分析方法，例如商业模式分析和护城河审计。它可以被直接执行一次，也可以作为方法说明预加载进 Agent，本身没有独立任务生命周期。
- `agent`：拥有独立 instructions、模型档位、上下文权限、精确版本 Skill 列表、只读 Tool 白名单和最大轮次的有界分析会话，例如公司深度分析和反方分析。

Skill 和 Agent 只生成只读结构化产物。公司看法、交易和投资规则的写入仍必须生成独立确认卡，并由确定性领域代码执行。

## 运行流程

一轮对话按以下顺序运行：

1. 确认本轮主题并生成确定性的 `ToolPlan`。
2. 先运行需要的原生研究工具。
3. 冻结本轮主题、用户问题、公司看法、研究来源、附件等授权上下文。
4. 对同一份冻结上下文并行运行匹配的 Skill 与 Agent。Skill 直接形成结构化产物；Agent 加载注册方法后，在“调用一个白名单只读 Tool”与“提交最终结构化结果”之间逐轮决策。主分析 Agent 已预加载的 Skill 默认不再独立运行，除非用户用 `@skill_id` 显式要求单独产物；反方 Agent 不读取其他能力的输出，避免被正方结论锚定。
5. 把通过输出 Schema 校验的产物交给回复模型综合，并随 assistant 消息持久化。
6. 回复后另行提取可确认的数据动作。

每个调用都发送持久化的 `tool.started`、`tool.progress`、`tool.completed` 或 `tool.failed` 事件。活动运行还返回尚未终止的调用快照，前端据此显示正在执行的能力，刷新、事件先于 REST 到达或断线重连后都可恢复。单个能力失败不会终止整轮对话，并会作为失败产物显示；对外只保存 `timeout`、`provider_error`、`invalid_output`、`unavailable` 等稳定错误类别，原始 provider/CLI 异常只写服务端日志。只有原生研究失败会进入“外部核验未完成”的研究降级提示。

Agent 的内部 Tool 调用不会绕过 Registry：模型只能返回 Tool id、精确版本和参数，Registry 再次校验白名单、输入 Schema、只读属性、自动确认属性、超时和输出 Schema。每轮最多调用一个 Tool，同一参数不能重复，单次 Agent 最多四次 Tool 调用并必须在最大轮次内保留最终回答。Tool 失败作为有界观察返回 Agent，使其可以降级完成，而不是开放新的权限。

Agent 决策的 provider Schema 不是开放对象，而是由运行时根据其白名单 Tool 输入 Schema 与最终产物 Schema 生成严格联合类型。因此 Codex CLI 的严格结构化输出与应用内 JSON Schema 校验使用同一组允许形状，未声明字段不能穿透 Registry。

## Prompt 与证据契约

模型能力的 Prompt 分三层组成：运行时协议负责安全、证据边界和严格 JSON；清单 instructions 只描述该 Skill/Agent 的角色、方法、研究策略与完成标准；输出 Schema 定义模型必须交付的字段。通用协议只要求 Schema 实际声明的字段，不假定所有自定义能力都使用内置分析结构。`research_company` 和 `research_community_insights` 是确定性 Rust Tool，没有模型 Prompt；它们只接收一个会改变判断的证据缺口，再由 `ResearchPlan` 生成受控检索。

四个内置模型能力使用不同的分析契约：

- `analyze_business_model` 覆盖产品与客户、价值与资金流、利润池与成本、单位经济与资本强度、所有者经济性、竞争强度、攻击者经济性和五至十年经营情景。
- `audit_moat` 覆盖候选结构机制、对竞争者的真实约束、维持代价、最低成本击穿路径及耐久性/失效条件，并先过滤“好产品、高份额、优秀管理”等虚假护城河替代物。
- `analyze_company` 综合商业模式、所有者经济性、竞争位置、护城河、管理与资本配置、财务韧性、盈利能力和最强失效机制。
- `challenge_company_thesis` 先强化正方论点，再只选择三至五个实质性失败机制；它不引用不存在的“上一版草稿”，也不为填满分类而制造通用风险。

内置产物先返回 `evidence_assessment`，状态为 `sufficient`、`partial` 或 `insufficient`。证据不足时不执行全分类填充，只返回简短拒绝、关键缺口、开放问题和最多两个用于指导下一步检索的低置信假设；证据部分充分时只输出有支持或会改变决策的类别，完整覆盖只适用于证据充分的分析。每条 finding 包含 `claim_type`、结构化 `evidence[]`、`causal_chain`、`counterargument`、`unknowns`、`confidence`、`leading_indicators`、`falsification` 和 `decision_impact`。标为 `fact` 的 finding 至少要有一个证据 URL；该 URL 必须与本轮冻结研究上下文或 Agent Tool 观察中的 URL 完全一致，否则确定性校验把产物记为 `invalid_output`。无法核验的判断必须降为 `inference` 或 `hypothesis`，不得用模型常识补市场份额、利润率、概率、攻击成本或远期盈利数字。社区来源只能提供待验证的争议、攻击路径和观察信号，不能证明事实或护城河。

Agent 先检查冻结证据，只在一个缺失事实足以改变重要结论时调用一个白名单 Tool；失败后不得换同义措辞重复同一路径。检索仍不可用时必须以 `partial` / `insufficient` 完成。前端直接展示证据等级、事实类型、来源、因果链和证伪条件；旧消息中的字符串 `evidence[]` 仍按无来源的历史证据显示，不会因 Schema 升级丢失。

## 本地清单

自定义清单从共享本地根目录的 `capabilities/**/*.json` 加载。共享根目录与 `.env`、SQLite 和 `data/workspace` 相同，默认指向 Git common dir 对应的原仓库，因此不同 worktree 使用同一份能力配置；`PRUDENTIA_LOCAL_DIR` 可以整体覆盖该根目录。`capabilities/` 被 Git 忽略，避免个人方法和提示词意外提交。

清单在后端启动时加载。新增或修改清单后需要重启后端。`GET /api/conversation/capabilities` 可以检查实际注册的名称、版本、类型、模型档位、适用主题和 surface；该接口不会返回能力 instructions。

一个最小的公司反方 Agent：

```json
{
  "id": "audit_customer_concentration",
  "version": 1,
  "kind": "agent",
  "stage": "challenge",
  "display_name": "客户集中度反方审计",
  "description": "检查客户集中、议价权和流失冲击",
  "artifact_type": "customer_concentration_audit",
  "instructions": "Use the supplied evidence to identify concentration mechanisms, counterarguments, unknowns, leading indicators, and falsification conditions.",
  "input_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": ["focus"],
    "properties": {
      "focus": { "type": "string", "maxLength": 4000 }
    }
  },
  "output_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": ["summary", "open_questions"],
    "properties": {
      "summary": { "type": "string", "maxLength": 6000 },
      "open_questions": {
        "type": "array",
        "maxItems": 12,
        "items": { "type": "string", "maxLength": 800 }
      }
    }
  },
  "context": ["subject", "user_message", "company_view", "research_sources", "attachments"],
  "model": "deep",
  "timeout_seconds": 300,
  "max_steps": 6,
  "tools": [
    { "id": "research_company", "version": 1 },
    { "id": "research_community_insights", "version": 1 }
  ],
  "skills": [
    { "id": "analyze_business_model", "version": 1 },
    { "id": "audit_moat", "version": 1 }
  ],
  "surfaces": ["conversation"],
  "subjects": ["company"],
  "triggers": ["客户集中度", "大客户风险", "customer concentration"],
  "initial_activity": "agent_auditing_customer_concentration"
}
```

## 清单契约

- `id`、`artifact_type`、`initial_activity` 使用最长 80 字节的 snake_case 标识符。
- `kind` 只能是 `skill` 或 `agent`；原生工具不能通过清单注入。
- `stage` 为 `analysis` 或 `challenge`，默认 `analysis`。执行形态与角色相互独立：Agent 可以承担主分析，Skill 也可以实现确定的反方方法；`research` 只保留给原生 Tool。
- `model` 为 `simple`、`standard` 或 `deep`，实际 provider/model 由当前 AI fallback chain 和模型档位配置决定。
- `skill` 的 `max_steps` 必须为 `1`；`agent` 最多为 `8`。每一步的实际 provider/model 都记录在产物 `model_steps` 中。
- 只有 Agent 可以声明 `tools` 和 `skills`。引用必须使用精确 `id + version`；最多八个 Tool、四个 Skill。依赖必须覆盖 Agent 声明的全部 surface 和 subject；依赖不存在、适用范围或类型不匹配、Tool 不是显式开放的只读自动执行能力时，整个 Agent 在启动注册阶段被忽略。
- `surfaces` 为 `conversation`、`rule_graph` 或两者；`subjects` 为 `company`、`investment_system`、`psychology`、`general` 的非空子集。
- 对话 surface 的输入必须只要求一个字符串字段 `focus`。planner 把本轮原始问题写入该字段。
- `context` 是显式最小权限列表，可选 `subject`、`user_message`、`company_view`、`research_sources`、`attachments`、`conversation_history`、`portfolio`、`investment_system` 和 `rule_graph_input`。声明 `rule_graph` surface 时必须显式授权 `rule_graph_input`，未授权时规则图不能把执行输入交给模型能力。
- JSON Schema 只支持运行时真正执行的子集：`type`、`enum`、`properties`、`required`、`additionalProperties`、`items`、`maxItems`、`maxLength` 和 `description`。未知关键字会拒绝整个清单，不能产生“写了但没有校验”的假安全。
- 一个清单最大 256 KiB，instructions 最大 32,000 字符，Agent 预加载的全部 Skill instructions 合计最大 48,000 字符，timeout 最大 600 秒；最多读取 64 个清单文件，目录深度最大 4，拒绝符号链接。
- 单个结构化输出最大 48 KiB；Agent 传回模型的观察总量最大 96 KiB，每次只带最多十二个来源且来源摘要截断到 1,600 字符。每轮最多运行三个模型 Capability，因此消息产物保持有界。工具事件和 `agent_trace` 只保存动作、版本、状态、轮次和来源数量，不重复保存研究正文；同一运行内相同 URL 的来源只保存一行并复用来源 ID。
- 单个无效清单会被记录并忽略，不影响同目录中的有效清单。目录结构或符号链接违反安全约束时，自定义目录整体拒绝加载。

对话 planner 对同一 `id` 选择最高版本，并根据主题与 `triggers` 确定是否运行；用户也可以用 `@capability_id` 显式调用。相同 `id + version` 不能覆盖已注册能力。版本升级应新增版本号，而不是改变旧版本语义。

## 规则图复用

声明了 `rule_graph` surface 的 Skill/Agent 会注册为精确版本 adapter，键格式为 `id@version`。规则节点示例：

```json
{
  "id": "customer-risk",
  "label": "客户集中度审计",
  "kind": "agent",
  "operation": "audit_customer_concentration",
  "config": {
    "adapter": "audit_customer_concentration@1",
    "arguments": { "focus": "检查当前输入中的客户集中风险" },
    "locale": "zh-CN"
  },
  "input_schema": { "type": "object", "required": ["context", "incoming"] },
  "output_schema": { "type": "object", "required": ["summary", "open_questions"] },
  "x": 320,
  "y": 180
}
```

激活规则图前会检查 adapter 存在、节点类型与 adapter 类型一致、Capability `arguments`、节点输入/输出数据形状和 DAG 无环性。每张图最多 64 个节点、256 条边和 8 个模型节点，序列化后最多 512 KiB；单次执行输入和单节点输出各有限额，整图最长运行 600 秒，最多保留最近 500 次执行。执行输入只保存一次，每个节点 trace 使用 `context_ref` 引用它并保存 incoming/output，避免重复放大存储。

激活时还会把清单内容哈希写入节点配置。执行要求 `id@version` 和 `manifest_hash` 同时一致；即使本地文件错误地原地修改了同一版本，已激活规则图也会拒绝使用，必须提升 Capability 版本并重新激活规则图。规则图不会自动漂移到最新版本。

## 扩展边界

普通分析方法优先使用 Skill；需要自主补证据、使用多个方法或独立形成判断的角色使用 Agent。需要访问新网站、数据库、文件格式或其他基础设施时，应新增经过审查的 Rust `native` adapter，并显式实现模型可调用输入转换、缓存、超时、存储和副作用策略。纯社区问题使用独立社区工具；同时要求商业模式、财务或护城河与社区观点时仍运行完整公司研究，避免用社区证据替代一手与独立来源。Prudentia 不提供 `load_skill(path)`、`spawn_agent(prompt)`、任意命令、Shell、浏览器或插件透传；CLI provider 只负责返回结构化 Agent 决策，所有工具执行和领域写入仍由应用自己的 Registry 与确认流程控制。
