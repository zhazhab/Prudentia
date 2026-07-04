import assert from "node:assert/strict";
import test from "node:test";
import {
  canCommitDraftRows,
  draftEditableDisplayFields,
  draftRowsForCommit,
  emptyPortfolioDraftRow,
  ensureDraftRowClientIds,
  formatBaseMoney,
  imageImportQueueCanStart,
  imageImportQueueRunningCount,
  marketOptionsForValue,
  mergeDuplicateDraftRowsBySymbol,
  performanceChartRows,
  performanceChartYAxisDomain,
  portfolioDashboardPanelIds,
  positionTableDisplayFields,
  portfolioImportFileKind,
  portfolioIssueLabel,
  positionEditDraft,
  supportedCurrencies,
  updateDraftRowField
} from "../src/pages/portfolioRules.ts";
import type {
  PortfolioDraftRow,
  PortfolioImageImportTask,
  PortfolioPerformanceResponse,
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

test("draft table display fields stay focused on manual import essentials", () => {
  assert.deepEqual(draftEditableDisplayFields, [
    "symbol",
    "name",
    "quantity",
    "average_cost",
    "currency",
    "market"
  ]);
});

test("position table display fields omit price freshness status", () => {
  assert.deepEqual(positionTableDisplayFields, [
    "symbol",
    "name",
    "market",
    "quantity",
    "average_cost",
    "market_value",
    "unrealized_pnl",
    "weight"
  ]);
});

test("portfolio dashboard panels stay focused on holdings only", () => {
  assert.deepEqual(portfolioDashboardPanelIds, ["positions"]);
});

test("draft row client identity survives editable value changes", () => {
  let nextId = 0;
  const [row] = ensureDraftRowClientIds([draftRow({ symbol: "" })], () => `draft-row:${++nextId}`);
  const edited = updateDraftRowField(row, "symbol", "0700") as PortfolioDraftRow & { client_row_id: string };
  const [next] = ensureDraftRowClientIds([edited], () => `draft-row:${++nextId}`);

  assert.equal(next.client_row_id, "draft-row:1");
  assert.equal(next.symbol, "0700");
  assert.equal(nextId, 1);
});

test("portfolio import file kind routes tabular files and images", () => {
  assert.equal(portfolioImportFileKind(fileLike("positions.csv", "")), "tabular");
  assert.equal(portfolioImportFileKind(fileLike("positions.tsv", "text/tab-separated-values")), "tabular");
  assert.equal(
    portfolioImportFileKind(
      fileLike("positions.xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
    ),
    "tabular"
  );
  assert.equal(portfolioImportFileKind(fileLike("positions.JPG", "")), "image");
  assert.equal(portfolioImportFileKind(fileLike("positions.webp", "image/webp")), "image");
  assert.equal(portfolioImportFileKind(fileLike("positions.pdf", "application/pdf")), "unsupported");
});

test("duplicate draft symbols merge before confirming", () => {
  const rows = mergeDuplicateDraftRowsBySymbol([
    draftRow({ symbol: "0700.HK", name: "Tencent", quantity: "100", average_cost: "300", currency: "HKD", market: "HK", imported_market_value: "32000" }),
    draftRow({ symbol: "0700.HK", name: "腾讯控股", quantity: "200", average_cost: "400", currency: "HKD", market: "HK", imported_market_value: "70000", last_price: "420" }),
    draftRow({ symbol: "MSFT" })
  ]);

  assert.equal(rows.length, 2);
  assert.equal(rows[0].symbol, "0700.HK");
  assert.equal(rows[0].quantity, "300");
  assert.equal(Number(rows[0].average_cost).toFixed(6), "366.666667");
  assert.equal(rows[0].imported_market_value, "116000");
  assert.equal(rows[0].last_price, "386.666667");
  assert.equal(rows[0].errors.length, 0);
  assert.equal(canCommitDraftRows(rows), true);
  assert.deepEqual(draftRowsForCommit(rows).map((row) => row.symbol), ["0700.HK", "MSFT"]);
});

test("legacy duplicate symbol errors are cleared while organizing draft rows", () => {
  const rows = mergeDuplicateDraftRowsBySymbol([
    draftRow({
      symbol: "0700.HK",
      quantity: "100",
      average_cost: "300",
      currency: "HKD",
      market: "HK",
      errors: ["duplicate symbol must be merged before confirming"]
    }),
    draftRow({
      symbol: "0700.HK",
      quantity: "200",
      average_cost: "400",
      currency: "HKD",
      market: "港股",
      errors: ["duplicate holding identifier must be merged before confirming"]
    })
  ]);

  assert.equal(rows.length, 1);
  assert.equal(rows[0].market, "HK");
  assert.deepEqual(rows[0].errors, []);
  assert.equal(canCommitDraftRows(rows), true);
});

test("duplicate draft symbols with conflicting markets stay blocking", () => {
  const rows = mergeDuplicateDraftRowsBySymbol([
    draftRow({ symbol: "0700.HK", currency: "HKD", market: "HK" }),
    draftRow({ symbol: "0700.HK", currency: "USD", market: "US" })
  ]);

  assert.equal(rows.length, 1);
  assert.equal(canCommitDraftRows(rows), false);
  assert.deepEqual(rows[0].errors, [
    "currency must match across duplicate symbol rows",
    "market must match across duplicate symbol rows"
  ]);
});

test("nonblank draft rows without symbols are blocking errors", () => {
  const edited = updateDraftRowField(draftRow({ symbol: "", name: "Tencent" }), "symbol", "");
  assert.deepEqual(edited.errors, ["symbol is required"]);

  const rows = mergeDuplicateDraftRowsBySymbol([
    draftRow({ symbol: "", name: "Tencent", account: "Main" }),
    draftRow({ symbol: "", name: " tencent ", account: "IRA" }),
    draftRow({ symbol: "", name: "Apple" })
  ]);

  assert.equal(canCommitDraftRows(rows), false);
  assert.deepEqual(
    rows.slice(0, 2).map((row) => row.errors),
    [["symbol is required"], ["symbol is required"]]
  );
  assert.deepEqual(rows[2].errors, ["symbol is required"]);
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

test("portfolio selectors expose supported currencies and preserve existing markets", () => {
  assert.deepEqual(supportedCurrencies, ["CNY", "HKD", "USD"]);
  assert.deepEqual(marketOptionsForValue("US").map((option) => option.value), [
    "A股",
    "港股",
    "美股",
    "Other",
    "US"
  ]);
});

test("portfolio image warnings are translated into user-facing issue labels", () => {
  assert.equal(
    portfolioIssueLabel("Symbol and currency are not visible in the screenshot.", "zh"),
    "截图里看不到代码和币种；请在表格里补齐后再确认。"
  );
  assert.equal(
    portfolioIssueLabel(
      "Currency and sector are not visibly shown in the screenshot. Only visible holding rows were extracted from the screenshot. Symbols/tickers, account, sector, and most markets were not visible.",
      "zh"
    ),
    "截图里有些字段看不清。我只提取了可见持仓；缺失字段请在表格里补齐。"
  );
  assert.equal(portfolioIssueLabel("average_cost must be a number", "zh"), "平均成本需要是数字。");
  assert.equal(portfolioIssueLabel("symbol is required", "zh"), "请填写代码。");
});

test("portfolio performance chart rows support return, annualized, and excess dimensions", () => {
  const performance = performanceResponse();

  assert.deepEqual(performanceChartRows(performance, "cumulative")[1], {
    label: "02/01",
    portfolio: 10,
    sp500: 5,
    hang_seng: -2
  });
  assert.deepEqual(performanceChartRows(performance, "annualized")[1], {
    label: "02/01",
    portfolio: 120,
    sp500: 60,
    hang_seng: -24
  });
  assert.deepEqual(performanceChartRows(performance, "excess")[1], {
    label: "02/01",
    sp500: 5,
    hang_seng: 12
  });
  assert.deepEqual(performanceChartYAxisDomain(performanceChartRows(performance, "excess")), [-1.44, 13.44]);
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
    last_price: null,
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

function fileLike(name: string, type: string): Pick<File, "name" | "type"> {
  return { name, type };
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

function performanceResponse(): PortfolioPerformanceResponse {
  return {
    period: "month",
    base_currency: "CNY",
    start_date: "2026-01-01T00:00:00Z",
    end_date: "2026-02-01T00:00:00Z",
    partial_period: false,
    portfolio: {
      start_value_base: 100,
      end_value_base: 110,
      profit_loss_base: 10,
      return_pct: 0.1,
      annualized_return_pct: 1.2
    },
    series: [
      {
        captured_at: "2026-01-01T00:00:00Z",
        value_base: 100,
        profit_loss_base: 0,
        return_pct: 0,
        annualized_return_pct: 0
      },
      {
        captured_at: "2026-02-01T00:00:00Z",
        value_base: 110,
        profit_loss_base: 10,
        return_pct: 0.1,
        annualized_return_pct: 1.2
      }
    ],
    benchmarks: [
      {
        key: "sp500",
        label: "S&P proxy",
        symbol: "SPY",
        available: true,
        stale: false,
        start_value_base: 100,
        end_value_base: 105,
        return_pct: 0.05,
        annualized_return_pct: 0.6,
        error: null,
        series: [
          {
            captured_at: "2026-01-01T00:00:00Z",
            value_base: 100,
            return_pct: 0,
            annualized_return_pct: 0,
            stale: false,
            error: null
          },
          {
            captured_at: "2026-02-01T00:00:00Z",
            value_base: 105,
            return_pct: 0.05,
            annualized_return_pct: 0.6,
            stale: false,
            error: null
          }
        ]
      },
      {
        key: "hang_seng",
        label: "Hang Seng proxy",
        symbol: "2800.HK",
        available: true,
        stale: false,
        start_value_base: 100,
        end_value_base: 98,
        return_pct: -0.02,
        annualized_return_pct: -0.24,
        error: null,
        series: [
          {
            captured_at: "2026-01-01T00:00:00Z",
            value_base: 100,
            return_pct: 0,
            annualized_return_pct: 0,
            stale: false,
            error: null
          },
          {
            captured_at: "2026-02-01T00:00:00Z",
            value_base: 98,
            return_pct: -0.02,
            annualized_return_pct: -0.24,
            stale: false,
            error: null
          }
        ]
      }
    ],
    updated_at: "2026-02-01T00:00:00Z"
  };
}
