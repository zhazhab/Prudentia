# Portfolio Import

[中文](portfolio-import.md)

The current import flow is:

1. Opening the import tools shows the shared draft table; users can add rows manually or use "Add file" for CSV/TSV/XLSX or PNG/JPG/JPEG/WebP.
2. The frontend routes the selected file by type to either CSV/Excel preview import or screenshot recognition; one selection can contain either one CSV/Excel file or a group of screenshots.
3. Files can append to or replace the current draft; screenshots append to the current draft as recognition tasks finish.
4. The backend refreshes the local code directory automatically in the background with the no-account public provider by default; users can run "Match codes" on the current draft, and the system only queries local `security_symbols` without live external search during import.
5. The user fixes draft fields, removes incorrect rows, and confirms the import; draft rows with the same `symbol` are merged automatically.
6. Backend merge-upserts by `symbol` without deleting existing holdings absent from the current draft.
7. Backend recomputes market value, unrealized P/L, return rates, CNY-based weights, and the portfolio summary, then records a portfolio performance snapshot and current position snapshots.

## Screenshot Recognition Drafts

The Portfolio page supports PNG, JPG/JPEG, and WebP screenshots uploaded through "Add file". Screenshot recognition uses the shared AI WebSocket and the configured Codex CLI provider to extract visible holding rows into editable draft rows. Multiple screenshots can run as cancelable tasks and append into the same draft table.

Screenshots and files share the same draft confirmation flow:

- It does not create import history.
- Nothing is written to SQLite before confirmation.
- The draft table shows only the six core editable fields: `symbol`, `name`, `quantity`, `average_cost`, `currency`, and `market`.
- Draft rows are merged automatically by `symbol`; quantity is added and average cost is weighted by quantity. When a visible current/last price is available, native market value is computed as `last_price × quantity`; otherwise the screenshot market-value column is used.
- Confirmation merge-upserts positions by `symbol`.
- Screenshot drafts may temporarily omit `symbol`; the system first looks for a unique existing holding by name, market, and currency, then falls back to the local code directory. Screenshot preview or the "Match codes" action fills in codes that can be determined. Before confirmation, the frontend still blocks nonblank rows without symbols.
- ETF/fund/security rows whose names contain "cash" are kept when they have holding metrics such as quantity, cost/current price, and market value, instead of being filtered as pure cash balances.
- The backend still rejects missing `symbol` values, currency/market conflicts, and other invalid fields during submission to avoid incorrect holdings.
- Existing holdings that are absent from the current draft are not deleted.

Users must verify recognized rows manually. Hidden rows, totals, and fields that are not visible in the screenshot are not inferred.

## Required Fields

- `name`
- `symbol`
- `quantity`
- `average_cost`
- `currency`

`symbol` is the final holding identifier. File imports should usually provide it directly; screenshot imports may leave it blank when the code is not visible, but users must fill it before confirming the draft.

## Local Code Directory

Prudentia uses existing holdings and the local `security_symbols` table for deterministic name-to-code matching. When a draft row omits `symbol`, the backend first matches existing holdings by name, market, and currency; only a unique match inherits the existing holding's `symbol`. If existing holdings cannot determine the code, it falls back to local `security_symbols`. The default `SYMBOL_DIRECTORY_PROVIDER=public` imports the normalized in-repo inventory file `data/symbol-directory/public/symbols.json` at backend startup. That file is generated from the no-account public directories declared in `config/symbol-directory-public.json`, including SSE stock/fund suggestion data, HKEX English and Traditional Chinese securities lists, and Nasdaq Trader US symbol lists. Each security record only keeps `symbol`, `name`, `market`, and `currency`; SQLite `security_symbols` only adds the file-level `updated_at`. Traditional Chinese security names are cleaned to Simplified Chinese before the inventory is written. Backend startup checks the inventory `updated_at`; the file is reused for 24 hours by default, and public sources are refreshed asynchronously only after expiry before replacing it. Failures only emit warnings and do not block startup or existing local matching. Local matching does not call Yahoo or other external search services live.

Chinese-name matching folds Traditional and Simplified variants, so simplified screenshot names can match HKEX Traditional Chinese entries. Short Hong Kong numeric codes normalize to the internal `0700.HK` form, so `700`, `0700`, and `00700.HK` all match the same code. Public coverage still follows source directories; when matching fails, users need to fill `symbol` manually or use a future authorized provider. You can also set `SYMBOL_DIRECTORY_PROVIDER=tushare` with `TUSHARE_TOKEN` to use Tushare basic data.

If the same name exists in multiple markets, such as dual-listed A/H companies, Prudentia only auto-matches when market or currency hints disambiguate the result. Otherwise the user must fill `symbol` manually in the draft table.

## Optional Fields

- `account`
- `market`
- `sector`
- `imported_market_value`
- `notes`

If a screenshot or file provides `last_price`, Prudentia computes initial native market value as `last_price × quantity`. Without `last_price`, it derives the initial last price from `imported_market_value / quantity`; without either value, average cost is used as the initial stale price until a valid market data refresh succeeds. The default mock quote provider does not overwrite imported market values; it only marks prices stale.

## Markets, Currencies, And CNY View

Prudentia first infers the phase-one supported markets from `symbol`:

- `US`: common US tickers such as `AAPL` or `BRK.B`.
- `HK`: Hong Kong tickers such as `0700.HK` or numeric tickers up to five digits.
- `CN`: A-share tickers such as `600519`, `000001`, or `.SS` / `.SZ` suffixes.

Draft `market` and `currency` fields remain editable. Each market displays native market value in its own currency, while the fixed base currency `CNY` is used for total portfolio value and weights. The frontend formats money with ISO currency prefixes, such as `CNY 1,234.56`, `HKD 1,234.56`, and `USD 1,234.56`.

The market data provider refreshes both stock quotes and FX. `MARKET_DATA_PROVIDER` can be configured as a comma-separated fallback chain, such as `yahoo,tencent` or `longbridge,yahoo`; current providers are mock, Yahoo Finance, Tencent quote, Longbridge OpenAPI, and Alpha Vantage-compatible. The mock provider is for offline development only and does not update holding or benchmark returns as available data; the Tencent provider handles stock quotes and queries Yahoo currency pairs through its FX adapter; the Alpha Vantage provider uses `CURRENCY_EXCHANGE_RATE`. If FX refresh fails, the last successful rate is retained and marked stale.

## Automatic Updates

Backend startup and background jobs check a daily TTL before refreshing quotes, FX, and benchmarks, so external providers are not called again within 24 hours by default. The holdings-page manual refresh forces one refresh, but still goes through provider-level minimum request spacing, 429/rate-limit cooldowns, and fallback degradation; Tencent quote and Longbridge batch same-cycle holding/benchmark quote requests. Benchmark snapshots are written only during the same holding price-refresh run, not during import, edit, or delete actions. Refresh failures only emit warnings and keep stale state. The frontend rereads local API data when entering the page, on window focus, after import/edit/delete operations, and after a manual refresh.

Automatic updates refresh quote-derived fields only:

- `last_price`
- `market_value`
- `unrealized_pnl`
- `weight`
- `price_updated_at`
- `price_stale`

Holding returns use the common broker holdings-page convention: native market value is `last_price × quantity`, native unrealized P/L is `(last_price - average_cost) × quantity`, and CNY unrealized P/L sums each holding's native unrealized P/L after FX conversion. Per-holding returns do not treat portfolio-level buy/sell changes as return adjustments.

Import confirmation, draft confirmation, position edits, and position deletes all record portfolio performance snapshots and current position snapshots; when these actions change the CNY portfolio value, the system automatically records a `buy` or `sell` trade adjustment. Daily quote refreshes only write snapshots and never create trade adjustments; benchmark snapshots are written only during the holding price-refresh cycle. The Portfolio Performance `this month`, `this year`, and `since inception` views read trade adjustments in the same period and calculate portfolio return as time-weighted return: each snapshot interval uses `(ending value - interval net trade adjustment) / starting value - 1`, then the interval returns are compounded; amount P/L is `end_value_base - start_value_base - net_cash_flow_base`; the unadjusted `simple_return_pct` is also returned as an explanation field. Annualized return is based on the time-weighted cumulative return; a single starting snapshot with zero return displays 0. If no snapshot exists at the period boundary, the UI displays the first available snapshot date. The holdings table reuses the same period selector and displays period return from each holding's CNY value snapshot change; current return rate still uses native unrealized P/L divided by cost basis.

Benchmark comparisons use S&P ETF proxy `SPY`, Hang Seng ETF proxy `2800.HK`, and the official SSE Composite `000001.SS`. Fetch failures only mark the benchmark unavailable/stale and do not block portfolio performance reads. The frontend supports cumulative return, annualized return, and relative excess return dimensions; excess return is displayed as portfolio cumulative return minus the benchmark cumulative return.

Position quantity and cost basis remain controlled by import/manual updates. Broker transaction sync is intentionally out of scope for v1.
