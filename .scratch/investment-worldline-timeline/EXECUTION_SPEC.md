# Decision Delta Timeline Execution Spec

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this spec task-by-task.

**Goal:** Build Decision Delta Timeline v1, where each quantifiable decision has an actual leg, a baseline shadow leg, and refreshed snapshots that show decision-level portfolio delta.

**Architecture:** Add a backend `decision_delta` module that owns decision leg persistence, snapshot calculation, and timeline summary behavior. Extend the existing `decision` module for list/get/update of decision inputs, reuse `MarketDataProvider` for quote/FX, and add a frontend Decisions/Timeline page with tested pure formatting and baseline rules.

**Tech Stack:** Rust, Axum, SQLx, SQLite, existing market data providers, React, TypeScript, TanStack Query, lucide-react, existing Node test runner.

---

## Current Codebase Facts

- `backend/src/decision/mod.rs` only supports `POST /api/decisions/` and stores free-text `action`, optional `symbol`, `confidence`, `expected_outcome`, and optional `review_date`.
- `backend/src/database.rs` owns handwritten SQLite migrations through `CREATE TABLE IF NOT EXISTS`.
- `backend/src/portfolio/mod.rs` already defines `BASE_CURRENCY = "CNY"`, portfolio FX rate storage, summary calculations, and price refresh flow.
- `backend/src/market_data/mod.rs` exposes `quote(symbol)` and `exchange_rate(from_currency, to_currency)`.
- `frontend/src/App.tsx` and `frontend/src/components/AppShell.tsx` use an enum-like `ViewKey` and hardcoded nav list.
- Frontend rule tests already exist under `frontend/test/*.test.ts` and are a good seam for pure timeline/delta formatting behavior.

## File Map

- Modify `backend/src/database.rs`: add tables for decision legs, decision snapshots, and optional decision review notes.
- Modify `backend/src/decision/mod.rs`: add list/get routes and support quantifiable decision input fields without breaking existing create callers.
- Create `backend/src/decision_delta/mod.rs`: own leg model, validation, snapshot calculation, timeline query, refresh orchestration, and routes.
- Modify `backend/src/lib.rs`: export `decision_delta`.
- Modify `backend/src/startup.rs`: mount `/api/decision-deltas`.
- Modify `backend/src/profile/mod.rs`: add rule-driven feedback for measurable decision tracking and reviews, not positive returns.
- Modify `backend/tests/framework.rs`: add service/API tests for legs, snapshots, stale flags, summary labeling, and compatibility.
- Modify `frontend/src/types/domain.ts`: add Decision, DecisionDeltaLeg, DecisionDeltaSnapshot, DecisionDeltaTimelineItem, summary, and request types.
- Modify `frontend/src/api/client.ts`: add decision list/get/update and decision delta timeline/refresh calls.
- Create `frontend/src/pages/decisionDeltaRules.ts`: pure baseline labels, badge formatting, filter/sort helpers, and summary text rules.
- Create `frontend/test/decisionDeltaRules.test.ts`: pure frontend rule tests.
- Create `frontend/src/pages/DecisionTimelinePage.tsx`: timeline, summary, selected decision comparison, qualitative notes panel, and refresh action.
- Modify `frontend/src/App.tsx`, `frontend/src/components/AppShell.tsx`, and `frontend/src/i18n.tsx`: add navigation and bilingual copy.
- Modify docs: `README.md`, `README.en.md`, `docs/api.md`, `docs/api.en.md`, `docs/architecture.md`, `docs/architecture.en.md`, `CHANGELOG.md`, and `CHANGELOG.en.md`.

## Data Model

Add `decision_delta_legs`:

- `id TEXT PRIMARY KEY`
- `decision_id TEXT NOT NULL`
- `leg_kind TEXT NOT NULL`: `actual` or `baseline`
- `baseline_type TEXT`: `cash`, `continue_holding`, `hypothetical_buy`, or `none`
- `symbol TEXT`
- `quantity REAL`
- `notional REAL`
- `price REAL`
- `currency TEXT NOT NULL`
- `created_at TEXT NOT NULL`
- `updated_at TEXT NOT NULL`
- foreign key `decision_id` references `decisions(id)` with cascade delete

Add `decision_delta_snapshots`:

- `id TEXT PRIMARY KEY`
- `decision_id TEXT NOT NULL`
- `as_of_date TEXT NOT NULL`
- `actual_value REAL NOT NULL`
- `baseline_value REAL NOT NULL`
- `delta_value REAL NOT NULL`
- `delta_pct REAL`
- `portfolio_impact_pct REAL`
- `price_used REAL`
- `price_source TEXT`
- `price_updated_at TEXT`
- `fx_rate_used REAL`
- `fx_source TEXT`
- `fx_updated_at TEXT`
- `price_stale INTEGER NOT NULL`
- `fx_stale INTEGER NOT NULL`
- `created_at TEXT NOT NULL`
- foreign key `decision_id` references `decisions(id)` with cascade delete

Add `decision_delta_reviews`:

- `decision_id TEXT PRIMARY KEY`
- `notes TEXT NOT NULL`
- `thesis_evidence_json TEXT NOT NULL`
- `disconfirming_evidence_json TEXT NOT NULL`
- `lessons_json TEXT NOT NULL`
- `candidate_principles_json TEXT NOT NULL`
- `candidate_checklist_items_json TEXT NOT NULL`
- `created_at TEXT NOT NULL`
- `updated_at TEXT NOT NULL`
- foreign key `decision_id` references `decisions(id)` with cascade delete

## API Contract

Extend Decisions:

- `GET /api/decisions/`
- `GET /api/decisions/{id}`
- `POST /api/decisions/` remains compatible with existing payloads.
- Extended create payload may include:
  - `decision_date`
  - `quantity`
  - `notional`
  - `price`
  - `currency`
  - `baseline_type`
  - `hypothetical_notional`

Add Decision Delta:

- `GET /api/decision-deltas/timeline`
  - Query: `symbol`, `action`, `year`, `delta`, `stale`, `reviewed`, `sort`
  - Returns `{ summary, items }`
- `POST /api/decision-deltas/refresh`
  - Body: optional `{ decision_ids: string[] }`
  - Refreshes quote/FX and writes new snapshots for quantifiable decisions.
- `GET /api/decision-deltas/{decision_id}`
  - Returns decision, legs, latest snapshot, snapshot history, and review.
- `PATCH /api/decision-deltas/{decision_id}/review`
  - Saves qualitative notes and candidate rules.
- `POST /api/decision-deltas/{decision_id}/adopt`
  - Manually adopts selected candidate principles/checklist items into Investment System.

## Calculation Rules

- `decision_delta = actual_leg_current_value - baseline_leg_current_value`.
- For asset legs: `current_value = quantity * latest_price * fx_to_base`.
- For cash legs in base currency: `current_value = notional`.
- For cash legs in non-base currency: `current_value = notional * fx_to_base`.
- `delta_pct = delta_value / abs(baseline_value)` when baseline value is non-zero; otherwise null.
- `portfolio_impact_pct = delta_value / portfolio_summary.total_market_value_base` when total base value is positive; otherwise null.
- Buy/add actual leg is asset exposure and baseline leg is retained cash.
- Sell/trim actual leg is cash from sale or reduced exposure and baseline leg is continued holding.
- Watch/skip is unquantifiable unless user supplies hypothetical notional and decision price.
- Refresh creates a new snapshot row; it does not overwrite history.
- Timeline uses latest snapshot per decision.
- Top summary is `sum of visible decision deltas`; UI and docs must not call it strict counterfactual Portfolio value.

## Implementation Tasks

### Task 1: Backend Schema And Decision Read APIs

**Files:**
- Modify `backend/src/database.rs`
- Modify `backend/src/decision/mod.rs`
- Test `backend/tests/framework.rs`

**Steps:**
- Add database tables exactly as described in Data Model.
- Add `GET /api/decisions/` and `GET /api/decisions/{id}`.
- Keep existing `POST /api/decisions/` payload compatible.
- Extend decision create model with optional quantification fields.
- If quantification fields are absent, create only the decision.
- If quantification fields are present, store them through Task 2 leg creation.
- Add tests:
  - existing create payload still succeeds
  - decision list returns created decision
  - decision get returns created decision
  - missing decision id returns not found

**Verification:**
- Run `cargo test -p prudentia-backend decision`
- Expected: new decision tests pass and existing decision/profile tests still pass.

### Task 2: Decision Leg Model And Validation

**Files:**
- Create `backend/src/decision_delta/mod.rs`
- Modify `backend/src/lib.rs`
- Modify `backend/src/startup.rs`
- Test `backend/tests/framework.rs`

**Steps:**
- Define Rust structs for `DecisionDeltaLeg`, `DecisionDeltaSnapshot`, `DecisionDeltaReview`, `DecisionDeltaTimelineSummary`, and `DecisionDeltaTimelineItem`.
- Implement validation for quantifiable decisions:
  - buy/add require symbol, quantity or notional, price, and currency
  - sell/trim require symbol, quantity, price, and currency
  - watch/skip require hypothetical notional before becoming quantifiable
- Implement leg creation:
  - buy/add creates actual asset leg and baseline cash leg
  - sell/trim creates actual cash leg and baseline continued-holding asset leg
  - watch/skip creates actual cash leg and hypothetical-buy baseline leg only when notional exists
- Mount routes under `/api/decision-deltas`.
- Add tests:
  - buy creates asset actual leg and cash baseline leg
  - sell creates cash actual leg and continued-holding baseline leg
  - skip without notional is marked unquantifiable
  - skip with notional creates cash actual leg and hypothetical-buy baseline leg

**Verification:**
- Run `cargo test -p prudentia-backend decision_delta`
- Expected: leg model tests pass.

### Task 3: Snapshot Calculation And Refresh

**Files:**
- Modify `backend/src/decision_delta/mod.rs`
- Test `backend/tests/framework.rs`

**Steps:**
- Implement current-value calculation for asset and cash legs.
- Reuse `MarketDataProvider::quote` for asset prices.
- Reuse `MarketDataProvider::exchange_rate` for non-base currency conversion.
- Reuse `portfolio::summary` for `portfolio_impact_pct`.
- Implement `POST /api/decision-deltas/refresh`.
- Persist one snapshot per quantifiable decision on each refresh.
- Preserve previous snapshots when a later refresh runs.
- Surface stale flags when quote or FX data is stale or unavailable; do not delete prior snapshots on provider failure.
- Add tests:
  - buy delta against cash baseline
  - sell delta against continued-holding baseline
  - non-base currency uses FX
  - refresh writes a new snapshot each time
  - provider failure keeps prior latest snapshot available with stale signal

**Verification:**
- Run `cargo test -p prudentia-backend decision_delta`
- Expected: snapshot and refresh tests pass.

### Task 4: Timeline Query And Summary

**Files:**
- Modify `backend/src/decision_delta/mod.rs`
- Test `backend/tests/framework.rs`

**Steps:**
- Implement `GET /api/decision-deltas/timeline`.
- Return latest snapshot per decision.
- Support filters for symbol, action, year, delta sign, stale state, and reviewed state.
- Support sort by date, absolute delta, portfolio impact, and stale state.
- Summary fields:
  - `visible_decisions_count`
  - `quantifiable_decisions_count`
  - `positive_delta_count`
  - `negative_delta_count`
  - `sum_delta_value`
  - `sum_portfolio_impact_pct`
  - `last_refreshed_at`
  - `label = "sum_of_decision_deltas"`
- Add tests:
  - summary sums only visible filtered decisions
  - summary label is not counterfactual Portfolio value
  - filters and sort return expected item order

**Verification:**
- Run `cargo test -p prudentia-backend decision_delta`
- Expected: timeline query tests pass.

### Task 5: Review Notes, Adoption, And Profile Feedback

**Files:**
- Modify `backend/src/decision_delta/mod.rs`
- Modify `backend/src/profile/mod.rs`
- Test `backend/tests/framework.rs`

**Steps:**
- Implement `PATCH /api/decision-deltas/{decision_id}/review`.
- Store notes, thesis evidence, disconfirming evidence, lessons, candidate principles, and candidate checklist items.
- Implement `POST /api/decision-deltas/{decision_id}/adopt`.
- Adoption only accepts candidates present in the saved review.
- Adoption appends through existing Investment System update behavior with dedupe.
- Update Profile rules to reward measurable decision tracking and review loops, not positive return.
- Add tests:
  - review save and update
  - adoption rejects non-candidate strings
  - adoption appends valid candidates
  - Profile XP/signals change after review count increases
  - Profile does not vary based on positive delta value alone

**Verification:**
- Run `cargo test -p prudentia-backend profile decision_delta`
- Expected: review/adoption/profile tests pass.

### Task 6: Frontend Types, API Client, And Pure Rules

**Files:**
- Modify `frontend/src/types/domain.ts`
- Modify `frontend/src/api/client.ts`
- Create `frontend/src/pages/decisionDeltaRules.ts`
- Create `frontend/test/decisionDeltaRules.test.ts`

**Steps:**
- Add TypeScript types matching the API contract.
- Add API client methods for decisions and decision deltas.
- Implement baseline labels:
  - buy/add: retained cash baseline
  - sell/trim: continued holding baseline
  - watch/skip without notional: unquantifiable
  - custom action: generic no-action baseline
- Implement delta badge formatting for positive, negative, zero, stale, and unquantifiable states.
- Implement summary label text as “sum of decision deltas”.
- Add tests covering all baseline labels, badge states, and summary wording.

**Verification:**
- Run `npm --prefix frontend run test`
- Expected: decision delta rule tests pass with existing frontend tests.

### Task 7: Decisions/Timeline Page

**Files:**
- Create `frontend/src/pages/DecisionTimelinePage.tsx`
- Modify `frontend/src/App.tsx`
- Modify `frontend/src/components/AppShell.tsx`
- Modify `frontend/src/i18n.tsx`
- Modify `frontend/src/styles/app.css`

**Steps:**
- Add nav item `Decisions` / `决策`.
- Add page route through existing `activeView` switch.
- Page layout:
  - top summary band
  - left timeline list with delta badges and stale markers
  - center selected-decision actual/baseline/delta comparison
  - right qualitative review panel
- Add refresh button that calls `POST /api/decision-deltas/refresh` and reloads timeline data.
- Add filters for symbol, action, delta sign, stale state, and reviewed state.
- Add sort control for date, absolute delta, portfolio impact, and stale state.
- Show explicit empty states:
  - no decisions
  - no quantifiable decisions
  - no snapshot yet
  - stale data
- Keep the UI dense and operational, consistent with the existing Prudentia dashboard style.

**Verification:**
- Run `npm --prefix frontend run build`
- Expected: TypeScript and Vite build succeed.

### Task 8: Documentation And Changelog

**Files:**
- Modify `docs/api.md`
- Modify `docs/api.en.md`
- Modify `docs/architecture.md`
- Modify `docs/architecture.en.md`
- Modify `README.md`
- Modify `README.en.md`
- Modify `CHANGELOG.md`
- Modify `CHANGELOG.en.md`

**Steps:**
- Document decision delta endpoints.
- Document shadow position as analytical, not real Portfolio position.
- Document that top summary is sum of visible decision deltas, not strict counterfactual Portfolio value.
- Document manual refresh/page refresh behavior.
- Document out-of-scope limits: no benchmark, no full counterfactual Portfolio simulation, no freeform worldline branching.
- Update changelog in Chinese and English.

**Verification:**
- Run `git diff --check`
- Expected: no whitespace errors.

## Final Verification

Run these from repo root after implementation:

```sh
cargo test -p prudentia-backend
npm --prefix frontend run test
npm --prefix frontend run build
git diff --check
```

Expected:

- backend tests pass
- frontend rule tests pass
- production frontend build passes
- diff check reports no whitespace errors

## Execution Order

1. Backend schema and decision read APIs.
2. Decision legs and validation.
3. Snapshot calculation and refresh.
4. Timeline query and summary.
5. Review/adoption/profile feedback.
6. Frontend types/API/rules.
7. Decisions/Timeline page.
8. Docs and changelog.

This order keeps each slice testable and prevents frontend work from guessing backend contracts.
