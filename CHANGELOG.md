# 变更日志

[English](CHANGELOG.en.md)

Prudentia 的重要变更都应记录在这里。最新条目插入当前版本段的最上方。

## 未发布

- 改进开发启动脚本：当前后端默认端口被占用时，`./scripts/dev.sh` 会自动选择可用端口，并把前端连接到实际后端地址。
- 优化 Settings 页 AI 配置：按 Mock、OpenAI-compatible、CLI provider 仅展示当前方式需要的输入项，并在保存时默认写入本地 `.env`。
- 优化 Decision Delta 性能：timeline 改为批量查询，detail snapshot 默认限制最近 90 条，refresh 复用 quote/FX，新增关键 SQLite 索引，并为 symbol filter 增加 debounce。
- 新增决策差异时间线：为 buy/add、sell/trim、watch/skip 决策生成 actual leg 与 baseline shadow leg，按最新行情刷新快照并展示可见决策收益差异汇总、单次分叉对比、复盘候选采纳和过程型 profile 奖励。
- 打磨 Portfolio 持仓体验：CSV/Excel/截图统一进入可编辑草稿表，确认后按 `symbol` 合并写入；新增持仓编辑/删除、US/HK/CN 市场推断、CNY 口径汇总，以及 market data provider FX 刷新和 stale fallback。
- 新增 Research Library，支持文章/人物投资思想蒸馏、股票快照分析、持仓组合复盘，并可将候选原则/checklist 写入投资体系。
- 在 README 中补充仓库名解释、愿景、理想目标和计划支持功能。
- 将双语文档拆分为独立文件：简体中文保留 `.md`，英文使用 `.en.md`。
- 将变更日志改为英文和简体中文双语可用。
- README 已增加简体中文，与英文并列维护。
- 记录 changelog 排序规则：最新条目插入当前版本段最上方。
- 记录开发完成规则：每次开发后更新 changelog；当启动方式、能力、公共接口或工作流变化时同步更新 README。
- 新增工程风格规范，覆盖可读性、维护性、可解释性、Rust 设计实践、泛型、enum、trait、注释和 review 要求。
- 新增 Codex CLI 作为第一个 CLI AI backend，并提供 `codex login --device-auth` device-code 登录说明。
- 新增 provider-based AI 与 market data 边界，包括 mock provider、OpenAI-compatible AI、Alpha Vantage-compatible market data，以及可复用的 CLI AI provider 层。
- 搭建本地优先的 Prudentia 项目框架，包括 Rust 后端、SQLite 持久化、React + Vite 前端、portfolio 导入、memo 工作流、投资体系编辑、画像反馈和双语 UI。
