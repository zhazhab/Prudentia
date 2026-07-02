import assert from "node:assert/strict";
import test from "node:test";
import {
  comparisonKindForAction,
  formatDecisionDeltaMoney,
  formatDecisionDeltaPercent,
  singleForkLegs,
  snapshotHistoryLimitLabel,
  summarizeVisibleDecisionDeltas
} from "../src/pages/decisionDeltaRules.ts";
import type {
  Decision,
  DecisionDeltaLeg,
  DecisionDeltaSnapshot,
  DecisionDeltaTimelineItem
} from "../src/types/domain.ts";

test("visible summary uses only latest snapshots for the current filtered timeline", () => {
  const summary = summarizeVisibleDecisionDeltas([
    item({ id: "buy", action: "buy" }, snapshot({ delta_value: 200 })),
    item(
      { id: "sell", action: "sell" },
      snapshot({ delta_value: -50, price_stale: true })
    ),
    item({ id: "watch", action: "watch" }, null, false)
  ]);

  assert.deepEqual(summary, {
    visibleDecisionsCount: 3,
    quantifiableDecisionsCount: 2,
    positiveDeltaCount: 1,
    negativeDeltaCount: 1,
    staleCount: 1,
    sumDeltaValue: 150,
    lastRefreshedAt: "2026-01-02T00:00:00Z"
  });
});

test("money and percent labels show portfolio-return difference direction", () => {
  assert.equal(formatDecisionDeltaMoney(200, "CNY"), "+CN¥200.00");
  assert.equal(formatDecisionDeltaMoney(-50, "CNY"), "-CN¥50.00");
  assert.equal(formatDecisionDeltaMoney(0, "CNY"), "CN¥0.00");
  assert.equal(formatDecisionDeltaPercent(0.125), "+12.5%");
  assert.equal(formatDecisionDeltaPercent(-0.02), "-2.0%");
  assert.equal(formatDecisionDeltaPercent(null), "n/a");
});

test("action comparison keeps each decision to a single actual versus baseline fork", () => {
  assert.equal(comparisonKindForAction("buy"), "asset_vs_cash");
  assert.equal(comparisonKindForAction("trim"), "cash_vs_holding");
  assert.equal(comparisonKindForAction("skip"), "cash_vs_hypothetical");
  assert.equal(comparisonKindForAction("rebalance"), "manual");

  const legs = singleForkLegs([
    leg({ leg_kind: "baseline", baseline_type: "cash" }),
    leg({ leg_kind: "actual", symbol: "AAPL", quantity: 10 })
  ]);

  assert.equal(legs.actual?.symbol, "AAPL");
  assert.equal(legs.baseline?.baseline_type, "cash");
});

test("snapshot history label communicates the visible history limit", () => {
  assert.equal(snapshotHistoryLimitLabel(90), "Latest 90 snapshots");
});

function item(
  decisionOverrides: Partial<Decision>,
  latestSnapshot: DecisionDeltaSnapshot | null,
  quantifiable = true
): DecisionDeltaTimelineItem {
  return {
    decision: decision(decisionOverrides),
    quantifiable,
    reviewed: false,
    latest_snapshot: latestSnapshot
  };
}

function decision(overrides: Partial<Decision> = {}): Decision {
  return {
    id: "decision-id",
    memo_id: null,
    symbol: "AAPL",
    action: "buy",
    rationale: "Track delta.",
    confidence: 0.7,
    expected_outcome: "Know the counterfactual.",
    review_date: null,
    created_at: "2026-01-01T00:00:00Z",
    ...overrides
  };
}

function snapshot(overrides: Partial<DecisionDeltaSnapshot> = {}): DecisionDeltaSnapshot {
  return {
    id: "snapshot-id",
    decision_id: "decision-id",
    as_of_date: "2026-01-02T00:00:00Z",
    actual_value: 1200,
    baseline_value: 1000,
    delta_value: 200,
    delta_pct: 0.2,
    portfolio_impact_pct: 0.01,
    price_used: 120,
    price_source: "test",
    price_updated_at: "2026-01-02T00:00:00Z",
    fx_rate_used: 1,
    fx_source: "identity",
    fx_updated_at: "2026-01-02T00:00:00Z",
    price_stale: false,
    fx_stale: false,
    created_at: "2026-01-02T00:00:00Z",
    ...overrides
  };
}

function leg(overrides: Partial<DecisionDeltaLeg> = {}): DecisionDeltaLeg {
  return {
    id: "leg-id",
    decision_id: "decision-id",
    leg_kind: "actual",
    baseline_type: null,
    symbol: null,
    quantity: null,
    notional: null,
    price: null,
    currency: "CNY",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides
  };
}
