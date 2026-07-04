# 工程风格

[English](engineering-style.en.md)

Prudentia 将代码可读性、可维护性和可解释性视为一等产品要求。代码应该让领域行为容易检查、评审，并能安全演进。

## 优先级

1. 优先清晰的领域建模，而不是聪明但隐晦的捷径。
2. 保持模块边界明确且足够小。
3. 在边界处明确输入、输出、错误情况和副作用。
4. 新增 provider 或工作流变体前，用测试锁定已有行为。

## Rust 后端

- 遵循 Rust 最佳实践处理 ownership、错误和 async 执行。业务规则优先使用显式 `Result`、类型化错误和小型纯函数。
- 用 `enum` 和结构化类型表达领域状态，避免 stringly typed 条件分支。外部字符串应在边界处解析，内部传递类型化值。
- 用 trait 作为可替换行为的接口，例如 AI provider、CLI backend、market data provider、broker 集成、导入解析器和未来的同步引擎。
- 当同一算法确实能复用于多个实现时使用泛型，例如 `CliAiProvider<B: CliBackend>`。不要为了显得抽象而引入泛型。
- 避免阻塞 Tokio async worker。CLI 调用、重文件解析和 CPU 密集转换应隔离在相应边界后。
- 避免大型编排函数。优先让领域模块拥有自己的校验、归一化、持久化调用和 provider 交互。
- 后端单个 Rust 源文件不得超过 800 行。超过限制时必须按领域逻辑拆分，例如类型、路由、导入解析、持久化、provider 适配和测试分别归位。`make check-backend-size` 会执行该限制。

## 拒绝面条代码

- 不要增长 god service 或跨模块长脚本。每个模块应该拥有一个清晰一致的领域概念。
- 当行为存在明确变体时，优先使用 `enum` + `match`、trait dispatch 或小型 policy object，而不是层层嵌套的 `if`。
- 不要重复 provider prompt schema、导入映射规则或 portfolio 计算逻辑。在变体分叉前抽取共享行为。
- 配置转换保持单向：环境变量/request 字符串只在边界转换为类型化 settings，后续代码消费类型化 settings。
- 有结构化 parser 或类型表示可用时，避免临时字符串解析。

## 注释与可解释性

- 为 invariant、非显然取舍、边界假设、单位、货币、时区行为和失败语义添加注释。
- 不要写重复代码字面含义的注释。
- Public trait、provider interface、request/response 边界类型应通过命名、类型和必要的简短 doc comment 自解释。
- 当函数接收外部输入时，应通过函数名、类型或短注释明确归一化和校验职责。

## 前端

- UI state 默认留在当前工作流内，除非它确实是共享应用状态。
- 使用 `types/domain.ts` 中的 typed API model，避免组件里出现无类型 JSON shape。
- 组件应该渲染一个工作流或可复用 UI primitive。当格式化、映射、API 编排开始遮蔽意图时，将其移出密集 JSX。
- 每个可见 UI 文案都要维护英文和简体中文。

## 文档本地化

- 默认 `.md` 文件使用简体中文。
- 英文文档放在成对的 `.en.md` 文件中。
- 当启动方式、能力、公共接口、工作流或规则语言变化时，成对文档必须同步维护。

## 完成标准

- 每次开发后更新 `CHANGELOG.md` 和 `CHANGELOG.en.md`。最新条目插入当前版本段最上方，覆盖用户可见行为、架构变化、provider 变化、API 变化、重要重构和纯文档规则变化。
- 当启动步骤、环境变量、命令、支持能力、公共接口或常见工作流变化时，同步更新 `README.md` 和 `README.en.md`。
- API、架构、导入模板等专项文档的责任边界变化时，同步更新对应语言文件。
- 最终说明中写清楚运行过哪些测试或构建；无法验证的内容要明确说明。

## Review Checklist

- 新贡献者是否能在不阅读整个代码库的情况下理解模块边界？
- 外部字符串是否在核心逻辑运行前转换为类型化值？
- 每个 provider 集成是否位于 trait 或窄接口之后？
- 泛型抽象是否承载真实复用，而不是仪式感？
- 注释是否解释边界和边缘情况，而不是复述语法？
- 行为变化是否有聚焦测试覆盖？
- 是否更新了 `CHANGELOG.md` 和 `CHANGELOG.en.md`？
- 如果用户可见的启动方式或工作流变化，是否更新了 `README.md` 和 `README.en.md`？
