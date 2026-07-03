import assert from "node:assert/strict";
import test from "node:test";
import {
  canCommitDraftRows,
  draftRowsForCommit,
  emptyPortfolioDraftRow,
  formatBaseMoney,
  imageImportQueueCanStart,
  imageImportQueueRunningCount,
  marketGroupsForDisplay,
  positionEditDraft,
  rowsWithDuplicateSymbolErrors,
  updateDraftRowField
} from "../src/pages/portfolioRules.ts";
import type {
  PortfolioDraftRow,
  PortfolioImageImportTask,
  PortfolioPosition,
  PortfolioSummary
} from "../src/types/domain.ts";

test("draft rows can be committed only when rows exist and no blocking errors remain", () => {
  assert.equal(canCommitDraftRows([]), false);
  assert.equal(canCommitDraftRows([draftRow({ errors: ["quantity must be greater than 0"] })]), false);
  assert.equal(canCommitDraftRows([draftRow({ confidence: "low", warnings: ["Low confidence"] })]), true);
});

test("blank manual draft rows are ignored before commit", () => {
  const blank = emptyPortfolioDraftRow();
  assert.equal(canCommitDraftRows([blank]), false);
  assert.equal(canCommitDraftRows([draftRow(), blank]), true);
  assert.deepEqual(draftRowsForCommit([blank, draftRow({ symbol: " msft " })]).map((row) => row.symbol), ["MSFT"]);
});

test("duplicate draft symbols are blocking errors", () => {
  const rows = rowsWithDuplicateSymbolErrors([
    draftRow({ symbol: "aapl", account: "Main" }),
    draftRow({ symbol: "AAPL", account: "IRA" }),
    draftRow({ symbol: "MSFT" })
  ]);

  assert.equal(canCommitDraftRows(rows), false);
  assert.deepEqual(
    rows.slice(0, 2).map((row) => row.errors),
    [["duplicate symbol must be merged before confirming"], ["duplicate symbol must be merged before confirming"]]
  );
  assert.deepEqual(rows[2].errors, []);
});

test("image import queue enforces a running limit", () => {
  const tasks: PortfolioImageImportTask[] = [
    imageTask({ status: "running" }),
    imageTask({ id: "second", status: "running" }),
    imageTask({ id: "third", status: "queued" })
  ];

  assert.equal(imageImportQueueRunningCount(tasks), 2);
  assert.equal(imageImportQueueCanStart(tasks, 2), false);
  assert.equal(imageImportQueueCanStart(tasks.slice(1), 2), true);
});

test("market groups display native values and cny weights in sorted order", () => {
  const groups = marketGroupsForDisplay({
    ...summary(),
    market_groups: [
      {
        market: "HK",
        currency: "HKD",
        market_value: 300,
        cost: 250,
        unrealized_pnl: 50,
        market_value_base: 270,
        weight: 0.135
      },
      {
        market: "US",
        currency: "USD",
        market_value: 100,
        cost: 90,
        unrealized_pnl: 10,
        market_value_base: 700,
        weight: 0.35
      }
    ]
  });

  assert.deepEqual(
    groups.map((group) => [group.label, group.nativeValue, group.weightLabel]),
    [
      ["US / USD", "$100.00", "35.0%"],
      ["HK / HKD", "HK$300.00", "13.5%"]
    ]
  );
});

test("base money is formatted as cny", () => {
  assert.equal(formatBaseMoney(summary({ total_market_value_base: 12345.67 })), "CN¥12,345.67");
});

test("position edit draft keeps editable fields as strings", () => {
  const draft = positionEditDraft({
    symbol: "AAPL",
    name: "Apple",
    asset_type: "stock",
    quantity: 2,
    average_cost: 100,
    currency: "USD",
    account: "Main",
    market: "US",
    sector: "Technology",
    notes: null,
    last_price: 125,
    market_value: 250,
    unrealized_pnl: 50,
    weight: 0.2,
    price_updated_at: null,
    price_stale: false,
    updated_at: "2026-01-01T00:00:00Z"
  });

  assert.deepEqual(draft, {
    name: "Apple",
    quantity: "2",
    average_cost: "100",
    currency: "USD",
    account: "Main",
    market: "US",
    sector: "Technology",
    imported_market_value: "250",
    notes: ""
  });
});

test("editing a draft row recomputes blocking errors", () => {
  const edited = updateDraftRowField(
    draftRow({ quantity: "-1", errors: ["quantity must be greater than 0"] }),
    "quantity",
    "3"
  );

  assert.deepEqual(edited.errors, []);
  assert.equal(edited.quantity, "3");
});

function draftRow(overrides: Partial<PortfolioDraftRow> = {}): PortfolioDraftRow {
  return {
    symbol: "AAPL",
    name: "Apple",
    quantity: "1",
    average_cost: "100",
    currency: "USD",
    account: null,
    market: "US",
    sector: null,
    imported_market_value: "100",
    notes: null,
    confidence: "high",
    warnings: [],
    errors: [],
    ...overrides
  };
}

function imageTask(overrides: Partial<PortfolioImageImportTask> = {}): PortfolioImageImportTask {
  return {
    id: "first",
    file_name: "positions.png",
    status: "queued",
    stage: null,
    elapsed_ms: 0,
    recognized_rows: 0,
    error: null,
    ...overrides
  };
}

function summary(overrides: Partial<PortfolioSummary> = {}): PortfolioSummary {
  return {
    total_market_value: 0,
    total_cost: 0,
    total_unrealized_pnl: 0,
    positions_count: 0,
    price_stale_count: 0,
    top_positions: [],
    sectors: [],
    market_groups: [],
    base_currency: "CNY",
    total_market_value_base: 0,
    total_cost_base: 0,
    total_unrealized_pnl_base: 0,
    fx_rates: [],
    fx_stale_count: 0,
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides
  };
}
