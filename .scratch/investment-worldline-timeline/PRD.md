# PRD: Decision Delta Timeline / 投资决策反事实收益树 v1

Status: ready-for-agent
Feature: investment-worldline-timeline
Tracker: local-markdown

## Problem Statement

Prudentia 的长期目标是帮助个人投资者把整个投资生涯沉淀成可复盘、可学习、可量化的资产。当前系统已经有 Memo、Research、Portfolio、Investment System、decision 和 Profile，但 decision 还主要是静态记录：它能说明用户当时做了什么，却不能持续回答“这次 decision 到今天为止给 Portfolio 增加或损失了多少价值”。

用户最关心的不是一棵无限展开的概念世界线，而是每一次投资 decision 相对“不行动 baseline”的可量化差异。例如：买入 AAPL 后，实际持有到今天的价值是多少；如果当时没买、保留现金，价值是多少；两者差额是多少；这个差额对整个 Portfolio 的收益贡献是多少。没有这个能力，复盘容易停留在 narrative 和结果记忆，无法看出长期收益到底来自哪些关键 decision。

原先的“做了 / 没做”文字树如果只做 qualitative review，价值偏弱。真正有产品价值的是 Decision Delta Timeline：每次 decision 生成实际腿和反事实影子腿，系统每天或每次刷新时用最新 quote / FX 重新计算两条腿的当前价值，并把 delta 展示在投资时间线上。

## Solution

Prudentia 应新增 Decisions/Timeline 工作台，作为投资生涯收益差异树的第一版入口。v1 的核心是：每个可量化 decision 生成一组 `actual leg` 和 `baseline shadow leg`，并持续计算两者之间的 `decision delta`。

对 buy/add decision，actual leg 是买入后持有的资产，baseline shadow leg 是当时没有买入而保留的现金。对 sell/trim decision，actual leg 是卖出或减仓后获得的现金与剩余实际仓位，baseline shadow leg 是如果当时没有卖出或没有减仓而继续持有的影子仓位。对 watch/skip decision，如果用户希望量化错过机会，需要输入 hypothetical notional；actual leg 是没有买入而保留现金，baseline shadow leg 是如果当时买入该金额会得到的影子仓位。

系统在刷新 quote / FX 后为每个 decision 计算 snapshot：actual value、baseline value、delta value、delta percentage、portfolio impact percentage、price used、FX used 和 stale flags。Timeline 上每个 decision 节点显示当前 delta badge。点开某个 decision 后，中间展示 actual leg、baseline shadow leg 和 delta summary；右侧展示 qualitative review、thesis、证据和可采纳的 Investment System 候选规则。

顶部总览显示当前可见 decisions 的 delta 合计、正贡献 decision 数、负贡献 decision 数和刷新时间。v1 不声称这是严格的 counterfactual Portfolio value，因为不同 decision 可能复用现金、相互依赖或重叠；它只展示 `sum of visible decision deltas`。

AI 在 v1 中不是主路径。AI 可以辅助解释 delta、整理 review note、提出候选 principles 和 checklist items，但不能替代 shadow leg 计算，也不能自动保存或自动改写 Investment System。收益差异来自 market data provider 和本地计算，解释层来自用户与可选 AI。

## User Stories

1. As an individual investor, I want to see my investment decisions as a timeline, so that my investment life becomes reviewable as a sequence of measurable choices.
2. As an individual investor, I want each decision to show its current delta versus a no-action baseline, so that I can see whether that choice has added or destroyed value.
3. As an individual investor, I want a buy decision to compare the current value of purchased shares against retained cash, so that I can quantify the value of buying.
4. As an individual investor, I want an add decision to compare the added shares against retained cash, so that I can quantify the value of increasing position size.
5. As an individual investor, I want a sell decision to compare the sale proceeds against continuing to hold the sold shares, so that I can quantify exit quality.
6. As an individual investor, I want a trim decision to compare the trimmed amount against continuing to hold it, so that I can quantify risk-reduction tradeoffs.
7. As an individual investor, I want a watch or skip decision to support a hypothetical notional amount, so that I can quantify missed opportunities only when I explicitly choose to.
8. As an individual investor, I want every quantifiable decision to have an actual leg and a baseline shadow leg, so that the calculation model is transparent.
9. As an individual investor, I want the baseline for buy/add to default to cash, so that the no-action scenario stays simple and auditable.
10. As an individual investor, I want the baseline for sell/trim to default to continued holding, so that exits are compared to what I actually gave up.
11. As an individual investor, I want the baseline for skip/watch to require user-entered notional, so that the system does not invent a position size.
12. As an individual investor, I want decision delta to update when prices or FX rates refresh, so that the tree stays alive over time.
13. As an individual investor, I want each decision snapshot to record the price and FX used, so that I can trust the calculation later.
14. As an individual investor, I want stale quote and stale FX flags on decision nodes, so that old data is not mistaken for current performance.
15. As an individual investor, I want each decision node to show delta value, delta percentage, and portfolio impact percentage, so that I can compare decisions at different scales.
16. As an individual investor, I want the timeline summary to show the sum of visible decision deltas, so that I can estimate the aggregate impact of a period or filter.
17. As an individual investor, I want the product to label the summary as a sum of decision deltas rather than a strict counterfactual Portfolio, so that I do not over-interpret the analysis.
18. As an individual investor, I want to filter the timeline by symbol, action, year, positive delta, negative delta, stale data, and reviewed state, so that I can find the important choices quickly.
19. As an individual investor, I want to sort decisions by absolute delta, portfolio impact, date, or stale state, so that high-impact choices surface naturally.
20. As an individual investor, I want to click a decision and see actual leg, baseline shadow leg, and delta summary side by side, so that the comparison is visually obvious.
21. As an individual investor, I want the actual leg to show quantity, price, value, currency, FX, and current value, so that I can audit the calculation.
22. As an individual investor, I want the baseline shadow leg to show baseline type, baseline quantity or cash, price, FX, and current value, so that the counterfactual leg is auditable.
23. As an individual investor, I want the delta panel to show actual value minus baseline value, so that there is one clear answer for each decision.
24. As an individual investor, I want qualitative notes beside the numeric delta, so that I can explain whether the result came from skill, luck, thesis drift, or risk control.
25. As an individual investor, I want to record thesis evidence and disconfirming evidence for a decision, so that high delta does not automatically mean high decision quality.
26. As an individual investor, I want to mark lessons and candidate rules from a decision, so that the numeric result can improve my Investment System.
27. As an individual investor, I want candidate principles and checklist items to require manual adoption, so that AI or calculations cannot silently rewrite my Investment System.
28. As an individual investor, I want Profile to reward completing review loops and maintaining measurable decision records, so that process quality is reinforced.
29. As an individual investor, I do not want Profile XP to be based directly on positive investment returns, so that the RPG layer does not reward outcome chasing.
30. As an individual investor, I want a negative delta decision to still support positive qualitative review, so that good risk process is not punished just because the market moved against me.
31. As an individual investor, I want a positive delta decision to still support warnings about poor process, so that luck is not mistaken for skill.
32. As an individual investor, I want daily snapshots to preserve history, so that I can see how a decision's impact evolved rather than only today's value.
33. As an individual investor, I want the latest snapshot to be easy to read by default, so that the main timeline stays useful.
34. As an individual investor, I want older snapshots to power charts later, so that the system can eventually show decision delta over time.
35. As an individual investor, I want manual refresh to work in v1, so that I can update deltas without needing background automation first.
36. As an individual investor, I want opening the page to optionally refresh stale data, so that the tree can stay current without feeling magical.
37. As an individual investor, I want all saved decisions, legs, and snapshots to live in local SQLite, so that the feature stays local-first.
38. As an individual investor, I want bilingual UI copy, so that the feature fits the existing English and Simplified Chinese experience.
39. As a developer, I want a stable decision leg model, so that buy/add/sell/trim/skip calculations are explicit and testable.
40. As a developer, I want snapshot calculation to be a backend service seam, so that API tests can verify behavior without depending on React internals.
41. As a developer, I want market data provider integration to reuse existing quote and FX refresh boundaries, so that this feature does not create a parallel pricing system.
42. As a developer, I want frontend graph and timeline helpers to be pure and tested, so that visual badges and detail panels stay consistent.
43. As a developer, I want AI explanation to be optional and separate from numeric calculation, so that provider failures do not block delta updates.
44. As a developer, I want docs to clearly distinguish decision delta from strict counterfactual Portfolio value, so that future agents do not overbuild the wrong model.

## Implementation Decisions

- The product capability is Decision Delta Timeline / 投资决策反事实收益树. “Worldline” remains a useful metaphor, but the implementation should center on measurable decision delta.
- v1 does not build an infinitely branching worldline tree. Each decision has one actual leg and one baseline shadow leg.
- Timeline is a list/tree of many decision nodes, and each decision node has a single two-leg comparison.
- Decision delta is calculated as `actual_leg_current_value - baseline_leg_current_value`.
- Buy/add actions use actual asset exposure versus baseline cash.
- Sell/trim actions use actual cash or reduced exposure versus baseline continued holding.
- Watch/skip actions are only quantifiable when the user supplies a hypothetical notional amount.
- Existing decision records should remain compatible, but quantifiable decision delta requires additional fields such as action type, symbol, decision date, quantity or notional, execution or decision price, currency, and baseline type.
- The system should introduce a first-class decision leg model rather than trying to infer all calculations from free-text rationale.
- The system should introduce daily or per-refresh decision snapshots with actual value, baseline value, delta value, delta percentage, portfolio impact percentage, price used, FX used, timestamp, and stale flags.
- Snapshots should be preserved as a history series rather than only overwriting a latest row.
- The latest snapshot is used for timeline badges and default detail display.
- The top-level summary is the sum of visible decision deltas, not a strict counterfactual Portfolio value.
- Strict counterfactual Portfolio simulation is deferred because it requires cash constraints, transaction ledgers, ordering rules, and alternative path assumptions.
- Market data refresh should reuse the existing market data provider boundary for quotes and FX.
- If quote or FX refresh fails, the system should keep the last known calculation visible with stale flags instead of silently hiding decision deltas.
- CNY remains the current base display currency where Portfolio summaries already use CNY; native leg currency should remain visible for auditability.
- Fees, taxes, dividends, splits, and corporate actions are out of scope for v1 unless existing provider data already supports them safely.
- AI-generated qualitative review is optional and subordinate to numeric delta calculation.
- AI must not invent prices, FX, quantities, fills, or external facts. Numeric fields come from persisted decision data and market data provider outputs.
- AI may help explain why a decision produced its current delta, summarize thesis evidence, and propose candidate principles or checklist items.
- AI preview does not write to persistence; user confirmation is required for saved qualitative review content.
- Investment System updates remain manual adoption of candidate principles and checklist items.
- Profile may reward review loops, measurable decision tracking, and disciplined follow-up, but not positive returns directly.
- Frontend should show a top summary, a timeline/tree of decision nodes with delta badges, a selected-decision actual/baseline/delta comparison, and a qualitative review panel.
- Frontend should not allow arbitrary graph node creation or arbitrary edge creation in v1.
- The UI should make data freshness visible on both timeline nodes and selected-decision details.
- Documentation must explain that “shadow position” is a per-decision analytical construct, not a real Portfolio position.
- Documentation must explain that summing decision deltas can double count capital when decisions overlap, so it is an analytical scorecard, not audited performance attribution.

## Testing Decisions

- Tests should verify external behavior at the highest practical seam: backend service/API behavior for decision legs, snapshots, and refresh; frontend pure rules for timeline badge and detail formatting.
- Backend tests should verify buy/add delta calculations against cash baseline.
- Backend tests should verify sell/trim delta calculations against continued-holding baseline.
- Backend tests should verify watch/skip decisions are not quantifiable without hypothetical notional.
- Backend tests should verify watch/skip decisions become quantifiable when hypothetical notional and decision price are provided.
- Backend tests should verify snapshot creation records actual value, baseline value, delta value, delta percentage, portfolio impact percentage, price used, FX used, timestamp, and stale flags.
- Backend tests should verify repeated refreshes preserve historical snapshots.
- Backend tests should verify latest snapshot selection for timeline display.
- Backend tests should verify stale quote and stale FX states are surfaced rather than hidden.
- Backend tests should verify failed provider refresh does not corrupt prior snapshots.
- Backend tests should verify top summary returns sum of visible decision deltas and does not label itself as strict counterfactual Portfolio value.
- Backend tests should verify existing create decision behavior remains compatible.
- Backend tests should verify quantifiable decision validation catches missing symbol, quantity/notional, price, currency, or baseline fields where required.
- Backend tests should verify AI explanation cannot supply numeric price or FX values into the calculation path.
- Frontend tests should verify action-to-baseline labeling for buy, add, sell, trim, watch, skip, and custom action states.
- Frontend tests should verify delta badge formatting for positive, negative, zero, stale, and unquantifiable decisions.
- Frontend tests should verify selected decision panels display actual leg, baseline shadow leg, and delta summary consistently.
- Frontend tests should verify summary labels say sum of decision deltas rather than counterfactual Portfolio value.
- Existing backend framework tests are the prior art for service/API behavior.
- Existing frontend Node test files for page rules are the prior art for pure frontend behavior.
- Verification for implementation should include backend tests, frontend tests, and frontend production build.

## Out of Scope

- Infinite branching worldline simulation is out of scope.
- Alternative path modeling such as “if I did not buy A, I bought B” is out of scope for v1.
- Strict full counterfactual Portfolio value is out of scope for v1.
- Cash-constrained portfolio-wide simulation is out of scope.
- Transaction ledger reconstruction is out of scope unless introduced by another feature.
- Broker sync is out of scope.
- Lot-level tax accounting is out of scope.
- Fees, taxes, dividends, splits, and corporate action handling are out of scope for v1.
- Benchmark comparison is out of scope for v1.
- User-defined benchmark baseline is out of scope for v1.
- Historical quote backfill beyond persisted snapshots is out of scope for v1.
- Background scheduled daily jobs are out of scope for v1; manual refresh or page-triggered refresh is enough.
- Automatic AI saving is out of scope.
- AI-driven price, FX, or quantity inference is out of scope.
- AI automatic Investment System mutation is out of scope.
- Return-based XP is out of scope.
- Freeform graph editing is out of scope.
- Arbitrary node and edge creation is out of scope.

## Further Notes

- The useful product is not a decorative “worldline tree”; it is a decision-level shadow-position scorecard that keeps updating.
- The core v1 formula is `decision_delta = actual_leg_current_value - baseline_shadow_leg_current_value`.
- The most important modeling constraint is to keep each decision local. Do not claim portfolio-wide counterfactual truth until the product has cash constraints and a transaction ledger.
- The local markdown tracker target is `.scratch/investment-worldline-timeline/PRD.md` with `Status: ready-for-agent`.
