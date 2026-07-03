# Portfolio Import

[中文](portfolio-import.md)

The current import flow is:

1. Add rows manually, upload CSV/TSV/XLSX, or upload/paste one or more screenshots.
2. Files can append to or replace the current draft; screenshots append to the current draft as recognition tasks finish.
3. User fixes draft fields, removes incorrect rows, merges duplicate symbols, and confirms the import.
4. Backend merge-upserts by `symbol` without deleting existing holdings absent from the current draft.
5. Backend recomputes market value, unrealized P/L, CNY-based weights, and the portfolio summary.

## Screenshot Recognition Drafts

The Portfolio page supports uploaded or pasted PNG, JPG/JPEG, and WebP screenshots. Screenshot recognition uses the shared AI WebSocket and the configured Codex CLI provider to extract visible holding rows into editable draft rows. Multiple screenshots can run as cancelable tasks and append into the same draft table.

Screenshots and files share the same draft confirmation flow:

- It does not create import history.
- Nothing is written to SQLite before confirmation.
- Confirmation merge-upserts positions by `symbol`.
- Duplicate `symbol` rows must be merged or removed before confirmation.
- Existing holdings that are absent from the current draft are not deleted.

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

## Markets, Currencies, And CNY View

Prudentia first infers the phase-one supported markets from `symbol`:

- `US`: common US tickers such as `AAPL` or `BRK.B`.
- `HK`: Hong Kong tickers such as `0700.HK` or numeric tickers up to five digits.
- `CN`: A-share tickers such as `600519`, `000001`, or `.SS` / `.SZ` suffixes.

Draft `market` and `currency` fields remain editable. Each market displays native market value in its own currency, while the fixed base currency `CNY` is used for total portfolio value and weights.

The market data provider refreshes both stock quotes and FX. The mock provider uses deterministic exchange rates; the Alpha Vantage provider uses `CURRENCY_EXCHANGE_RATE`. If FX refresh fails, the last successful rate is retained and marked stale.

## Automatic Updates

Automatic updates refresh quote-derived fields only:

- `last_price`
- `market_value`
- `unrealized_pnl`
- `weight`
- `price_updated_at`
- `price_stale`

Position quantity and cost basis remain controlled by import/manual updates. Broker transaction sync is intentionally out of scope for v1.
