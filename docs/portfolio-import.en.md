# Portfolio Import

[中文](portfolio-import.md)

The first import flow is:

1. Upload CSV, TSV, or XLSX in the frontend.
2. Backend parses headers and sample rows.
3. Backend suggests field mappings.
4. User confirms mappings.
5. Backend commits normalized positions into SQLite.
6. Backend recomputes market value, unrealized P/L, and portfolio weights.

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
