# Prudentia

[English](README.en.md)

Prudentia 是一个本地优先的个人投资工作台，用于搭建投资体系、记录 thesis 驱动的投资备忘录、展示 portfolio，并沉淀 RPG-like 的投资者画像。

## 仓库名

`Prudentia` 取意于 prudence 和 practical wisdom：审慎、实践智慧，以及在不确定环境中持续做出更好判断的能力。这个名字强调纪律、复盘和长期主义，而不是短期交易冲动。

## 愿景

Prudentia 希望成为个人投资者的本地优先投资操作系统。它帮助用户把零散研究、投资决策、持仓反馈和自我画像沉淀为可复盘资产，让每一次投资行为都成为长期能力的一部分。

## 理想目标

理想状态下，用户可以在 Prudentia 中形成自己的投资体系，记录每一次决策的假设、风险、催化剂、反证条件和复盘结果。系统会基于这些行为逐渐生成 RPG-like 投资画像，帮助用户理解自己的能力圈、决策纪律、风险偏好和常见偏差。

## 当前能力

- Rust 后端：`axum`、`sqlx`、SQLite、provider-based AI、provider-based market data。
- React + Vite + TypeScript 前端：Dashboard、Portfolio、Memos、Investment System、Profile。
- Portfolio CSV/Excel/截图统一草稿导入、字段映射、确认合并写入、持仓编辑/删除、市值/权重/盈亏计算、CNY 口径汇总、定时行情与 FX 刷新。
- Research Library：文章/人物投资思想蒸馏、股票快照分析、持仓组合复盘，以及候选原则/checklist 写入投资体系。
- 规则驱动的投资画像：XP、等级、徽章、属性和偏差信号。
- 支持英文和简体中文 UI，前端通过 `Accept-Language` 控制后端生成文本语言。

## 计划支持的功能

- 更完整的 memo 生命周期：观察、建仓、加仓、减仓、卖出、复盘和归档。
- Portfolio 导入增强：字段映射保存、重复导入处理、账户/市场/行业维度分析。
- 更多行情、AI 和 CLI provider，让外部服务可以通过清晰接口替换。
- 决策复盘提醒，把 review date 和 thesis 周期变成可执行工作流。
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

常用命令：

```sh
cargo fmt
cargo test -p prudentia-backend
```

## 前端

```sh
npm install --prefix frontend
npm --prefix frontend run dev
```

前端开发服务器默认监听 `http://127.0.0.1:5173`，并将 `/api` 代理到后端。

常用命令：

```sh
npm --prefix frontend run build
```

## Provider 默认配置

`AI_PROVIDER=mock` 和 `MARKET_DATA_PROVIDER=mock` 可以让应用在没有外部 API key 的情况下运行。

使用 Alpha Vantage-compatible 行情刷新：

```env
MARKET_DATA_PROVIDER=alpha_vantage
ALPHA_VANTAGE_API_KEY=your_key
```

使用 OpenAI-compatible chat completions endpoint：

```env
AI_PROVIDER=openai
OPENAI_API_KEY=your_key
OPENAI_BASE_URL=https://api.openai.com/v1
OPENAI_MODEL=gpt-4.1-mini
```

使用通用 CLI provider 接入 Codex CLI 和 ChatGPT/device-code 登录：

```sh
codex login --device-auth
```

然后配置：

```env
AI_PROVIDER=cli
AI_CLI_PROVIDER=codex
AI_CLI_PATH=codex
AI_CLI_MODEL=
AI_CLI_PROFILE=
```

也可以在应用的 Settings 页面编辑 AI 配置。运行时配置会立即生效；启用 “write .env” 后会写入 `.env`，后端重启后继续生效。

## 导入模板

首个支持的 portfolio 导入格式见 [examples/portfolio_import.csv](examples/portfolio_import.csv)。
