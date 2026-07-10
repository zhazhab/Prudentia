# Portfolio 导入

[English](portfolio-import.en.md)

当前导入流程：

1. 打开导入工具后显示同一张草稿表；用户可以手动“新增行”，或通过“新增文件”选择 CSV/TSV/XLSX 或 PNG/JPG/JPEG/WebP。
2. 前端按文件类型决定调用 CSV/Excel 预览导入或截图识别；一次选择中只能包含一个 CSV/Excel 文件，或一组截图。
3. 文件可以追加到当前草稿或替换当前草稿；截图识别任务完成后会追加到同一张草稿表。
4. 后端默认用免账号 public provider 自动后台刷新本地代码库；用户可以对当前草稿执行“匹配代码”，系统只查本地 `security_symbols`，不在导入时实时外部搜索。
5. 用户修正草稿中的字段、删除错误行，并确认导入；相同 `symbol` 的草稿行会自动归并。
6. 后端按 `symbol` 合并写入 SQLite，不删除本次未出现的旧持仓。
7. 后端重新计算 market value、unrealized P/L、收益率、CNY 口径权重和 portfolio summary，并写入组合收益快照和当前持仓快照。

## 截图识别草稿

Portfolio 页支持通过“新增文件”上传 PNG、JPG/JPEG、WebP 截图进行识别。截图识别通过统一 AI WebSocket 调用已配置的 Codex CLI provider，把截图中可见的持仓行提取为可编辑草稿。多张截图会作为可取消任务追加到同一张草稿表。

截图和文件共用同一套草稿确认流程：

- 不创建导入历史。
- 确认前不写入 SQLite。
- 草稿表默认只展示 `symbol`、`name`、`quantity`、`average_cost`、`currency`、`market` 六个核心编辑字段。
- 草稿会按 `symbol` 自动归并；数量相加，平均成本按数量加权。截图中能看到现价/当前价时，会优先用 `现价 × 数量` 计算本币市值；否则才使用截图市值列。
- 确认后按 `symbol` 合并更新或新增持仓。
- 截图草稿可以暂时缺少 `symbol`；系统会先按名称、市场和币种从当前已有持仓中找唯一匹配项，找不到时再查本地代码库。截图预览或用户点击“匹配代码”会补齐可确定的代码；确认前前端仍会阻断缺少代码的非空行。
- 名称包含“现金”的 ETF/基金/证券行如果有数量、成本/现价和市值等持仓指标，会作为持仓保留，不会按纯现金余额过滤。
- 后端仍会在提交阶段拒绝缺少 `symbol`、币种/市场冲突或其他无效字段，避免写入错误持仓。
- 不会删除本次草稿中没有出现的旧持仓。

识别结果需要用户自行核对。截图中的隐藏行、合计行或不可见字段不会被推断。

## 必填字段

- `name`
- `symbol`
- `quantity`
- `average_cost`
- `currency`

`symbol` 是最终持仓的唯一标识。文件导入通常应直接提供 `symbol`；截图导入如果看不到代码，可以先留空进入草稿，但确认前必须由用户补齐。

## 本地代码库

Prudentia 使用当前已有持仓和本地 `security_symbols` 表做名称到代码的确定性匹配。草稿缺少 `symbol` 时，会先按名称、市场和币种匹配当前已有持仓；只有唯一匹配时才继承已有持仓的 `symbol`。如果当前持仓无法确定，再查本地 `security_symbols`。默认 `SYMBOL_DIRECTORY_PROVIDER=public` 会在后端启动时先导入项目内标准化存量文件 `data/symbol-directory/public/symbols.json`。该文件由 `config/symbol-directory-public.json` 声明的公开免账号目录生成，包括上交所股票/场内基金、HKEX 英文与繁体中文证券列表和 Nasdaq Trader 美股列表；每条证券只保留 `symbol`、`name`、`market`、`currency`，SQLite `security_symbols` 只多保存文件级 `updated_at`。生成前会将繁体中文证券名称清洗为简体。后端启动时检查存量文件 `updated_at`，默认 24 小时内复用，过期后才异步刷新公开源并覆盖这份文件；失败只记录 warning，不阻塞启动或已有本地匹配。本地匹配不会实时请求 Yahoo 或其他外部搜索服务。

中文名称匹配会做简繁折叠，例如简体截图里的港股名称可以匹配 HKEX 繁体中文列表。短港股数字代码会规范为内部 `0700.HK` 形式，例如 `700`、`0700` 或 `00700.HK` 都会匹配同一代码。公开源覆盖仍以来源目录为准，匹配不到时需要用户手动补 `symbol`，或等待后续授权 provider 扩展。也可以把 `SYMBOL_DIRECTORY_PROVIDER` 改成 `tushare` 并配置 `TUSHARE_TOKEN` 使用 Tushare 基础数据。

如果同一名称在多个市场都有标的，例如 A/H 两地上市公司，系统只会在市场或币种提示能唯一消歧时自动匹配；否则用户需要在草稿表中手动填写 `symbol`。

## 可选字段

- `account`
- `market`
- `sector`
- `imported_market_value`
- `notes`

如果截图或文件提供了 `last_price`，Prudentia 会用 `last_price × quantity` 计算初始本币市值。没有 `last_price` 时，才会用 `imported_market_value` 除以 quantity 推导初始 last price；两者都没有时使用 average cost 作为初始 stale price，直到有效 market data refresh 成功。默认 mock 行情不会覆盖导入得到的市值，只会把价格标记为 stale。

## 市场、币种和 CNY 口径

Prudentia 会优先根据 `symbol` 自动推断第一阶段支持的市场：

- `US`：常见美股代码，例如 `AAPL`、`BRK.B`。
- `HK`：港股代码，例如 `0700.HK` 或 5 位以内纯数字代码。
- `CN`：A 股代码，例如 `600519`、`000001`、`.SS`、`.SZ` 后缀。

草稿里的 `market` 和 `currency` 始终可以手动编辑。每个市场按本市场币种展示 native 市值，同时固定使用 `CNY` 作为基础币种计算总持仓额和权重。前端金额展示统一使用 ISO 币种前缀，例如 `CNY 1,234.56`、`HKD 1,234.56`、`USD 1,234.56`。

Market data provider 会负责刷新股票报价和 FX。`MARKET_DATA_PROVIDER` 可配置为逗号 fallback 链，例如 `yahoo,tencent` 或 `longbridge,yahoo`；当前支持 mock、Yahoo Finance、腾讯行情、长桥 OpenAPI 和 Alpha Vantage-compatible。Mock provider 只用于离线开发，不会把持仓或 benchmark 收益更新为可用数据；腾讯 provider 负责股票报价，FX 通过 Yahoo 货币对 adapter 查询；Alpha Vantage provider 使用 `CURRENCY_EXCHANGE_RATE`。如果 FX 刷新失败，系统会保留最后成功汇率并标记为 stale。

## 自动更新

后端启动和后台任务会按每日 TTL 检查是否需要刷新行情、FX 和 benchmark，默认 24 小时内不重复请求外部 provider。持仓页的手动刷新会强制执行一次刷新，但仍受 provider 级最小请求间隔、429/频控冷却和 fallback 降级保护；腾讯行情和长桥会对同一轮持仓/benchmark 报价使用 batch 获取。Benchmark 只在同一轮持仓价格刷新里写入快照，不随导入、编辑或删除单独刷新。刷新失败只记录 warning，并保留 stale 状态；前端在进入页面、窗口聚焦和导入/编辑/删除后会重新读取本地 API 数据。

自动更新只刷新由行情派生的字段：

- `last_price`
- `market_value`
- `unrealized_pnl`
- `weight`
- `price_updated_at`
- `price_stale`

持仓收益按券商持仓页常见口径计算：单只持仓本币市值为 `last_price × quantity`，本币浮盈亏为 `(last_price - average_cost) × quantity`；组合 CNY 浮盈亏为各持仓本币浮盈亏按 FX 转换后的合计。单只持仓收益不把组合层面的买入/卖出变动计入收益率调整。

导入确认、草稿确认、持仓编辑和持仓删除都会写入组合收益快照和当前持仓快照；这些动作导致 CNY 组合市值变化时，系统会自动记录 `buy` 或 `sell` 交易调整。每日行情刷新只写快照，不产生交易调整；benchmark 快照只跟随持仓价格刷新周期写入。Portfolio Performance 的 `本月`、`本年`、`记录起` 视角会读取同周期交易调整，并按时间加权收益率计算组合收益：每个快照区间收益为 `(期末市值 - 区间净交易调整) / 期初市值 - 1`，再连乘得到累计收益率；金额收益为 `end_value_base - start_value_base - net_cash_flow_base`；同时返回未调整交易变动的 `simple_return_pct` 作为解释字段。年化收益基于时间加权累计收益率计算；只有一条起点快照且收益为 0 时显示 0。如果周期起点没有快照，UI 会显示“自 YYYY-MM-DD 起”。持仓表复用同一个周期选择器，按单只持仓快照的 CNY 市值变化展示周期收益率；当前收益率仍按本币浮盈亏除以持仓成本展示。

指数对比使用：标普 ETF 代理 `SPY`、恒生 ETF 代理 `2800.HK`、官方上证综指 `000001.SS`。抓取失败只会标记 unavailable/stale，不影响组合收益读取。前端支持累计收益、年化收益和相对指数超额收益三个对比维度；超额收益按组合累计收益率减去 benchmark 累计收益率展示。

Position quantity 和 cost basis 仍由导入或手动更新控制。Broker transaction sync 有意不纳入 v1 范围。
