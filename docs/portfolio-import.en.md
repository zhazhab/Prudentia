# Portfolio Import

[中文](portfolio-import.md)

The first import flow is:

1. Upload CSV, TSV, or XLSX in the frontend.
2. Backend parses headers and sample rows.
3. Backend suggests field mappings.
4. User confirms mappings.
5. Backend commits normalized positions into SQLite.
6. Backend recomputes market value, unrealized P/L, and portfolio weights.

## Screenshot Recognition Preview

The Portfolio page also supports uploaded or pasted PNG, JPG/JPEG, and WebP screenshots. Screenshot recognition uses the configured Codex CLI provider to extract visible holding rows into editable draft rows.

The first screenshot recognition version is preview-only:

- It does not write to SQLite.
- It does not update existing positions.
- It does not create import history.
- It does not recompute market value, unrealized P/L, or weight.

Users must verify recognized rows manually. Hidden rows, totals, and fields that are not visible in the screenshot are not inferred.

## Required Fields

- `symbol`
- `name`
- `quantity`
- `average_cost`
- `currency`

## Optional Fields

- `account`
- `market`
- `sector`
- `imported_market_value`
- `notes`

If `imported_market_value` is present, Prudentia derives an initial last price from market value divided by quantity. Otherwise, average cost is used as the initial stale price until a market data refresh succeeds.

## Automatic Updates

Automatic updates refresh quote-derived fields only:

- `last_price`
- `market_value`
- `unrealized_pnl`
- `weight`
- `price_updated_at`
- `price_stale`

Position quantity and cost basis remain controlled by import/manual updates. Broker transaction sync is intentionally out of scope for v1.
