use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use chrono::{Datelike, NaiveDate};
use reqwest::Url;
use serde_json::Value;

use crate::ai::ConversationResearchSource;

const REVENUE_TAGS: [(&str, &str); 4] = [
    (
        "us-gaap",
        "RevenueFromContractWithCustomerExcludingAssessedTax",
    ),
    ("us-gaap", "Revenues"),
    ("us-gaap", "SalesRevenueNet"),
    ("ifrs-full", "Revenue"),
];
const NET_INCOME_TAGS: [(&str, &str); 3] = [
    ("us-gaap", "NetIncomeLoss"),
    ("us-gaap", "ProfitLoss"),
    ("ifrs-full", "ProfitLoss"),
];
const GROSS_PROFIT_TAGS: [(&str, &str); 2] =
    [("us-gaap", "GrossProfit"), ("ifrs-full", "GrossProfit")];
const COST_OF_REVENUE_TAGS: [(&str, &str); 2] =
    [("us-gaap", "CostOfRevenue"), ("ifrs-full", "CostOfSales")];
const OPERATING_INCOME_TAGS: [(&str, &str); 2] = [
    ("us-gaap", "OperatingIncomeLoss"),
    ("ifrs-full", "ProfitLossFromOperatingActivities"),
];
const SELLING_MARKETING_TAGS: [(&str, &str); 1] = [("us-gaap", "SellingAndMarketingExpense")];
const OPERATING_CASH_FLOW_TAGS: [(&str, &str); 2] = [
    ("us-gaap", "NetCashProvidedByUsedInOperatingActivities"),
    ("ifrs-full", "CashFlowsFromUsedInOperatingActivities"),
];
const CAPITAL_EXPENDITURE_TAGS: [(&str, &str); 4] = [
    ("us-gaap", "PaymentsToAcquirePropertyPlantAndEquipment"),
    ("us-gaap", "PaymentsToAcquireProductiveAssets"),
    ("ifrs-full", "PurchaseOfPropertyPlantAndEquipment"),
    (
        "ifrs-full",
        "PurchaseOfPropertyPlantAndEquipmentClassifiedAsInvestingActivities",
    ),
];
const SHARE_BASED_COMPENSATION_TAGS: [(&str, &str); 3] = [
    ("us-gaap", "ShareBasedCompensation"),
    ("ifrs-full", "ShareBasedPayment"),
    ("ifrs-full", "ShareBasedPaymentTransactionExpense"),
];
const DILUTED_SHARES_TAGS: [(&str, &str); 2] = [
    ("us-gaap", "WeightedAverageNumberOfDilutedSharesOutstanding"),
    ("ifrs-full", "DilutedWeightedAverageShares"),
];
// Only a one-year 50x reversal is removed; persistent unit changes such as splits remain intact.
const ISOLATED_SCALE_OUTLIER_RATIO: f64 = 50.0;

pub(super) fn company_facts_url(filing_url: &str) -> Option<String> {
    let url = Url::parse(filing_url).ok()?;
    let segments = url.path_segments()?.collect::<Vec<_>>();
    let data_index = segments.iter().position(|segment| *segment == "data")?;
    let cik = segments.get(data_index + 1)?.parse::<u64>().ok()?;
    Some(format!(
        "https://data.sec.gov/api/xbrl/companyfacts/CIK{cik:010}.json"
    ))
}

pub(super) fn source_from_company_facts(
    body: &Value,
    url: &str,
    requested_years: usize,
) -> Option<ConversationResearchSource> {
    let revenue = extract_currency_metric(body, &REVENUE_TAGS);
    let gross_profit = extract_currency_metric(body, &GROSS_PROFIT_TAGS);
    let cost_of_revenue = extract_currency_metric(body, &COST_OF_REVENUE_TAGS);
    let operating_income = extract_currency_metric(body, &OPERATING_INCOME_TAGS);
    let net_income = extract_currency_metric(body, &NET_INCOME_TAGS);
    let selling_marketing = extract_currency_metric(body, &SELLING_MARKETING_TAGS);
    let operating_cash_flow = extract_currency_metric(body, &OPERATING_CASH_FLOW_TAGS);
    let capital_expenditure = extract_currency_metric(body, &CAPITAL_EXPENDITURE_TAGS);
    let share_based_compensation = extract_currency_metric(body, &SHARE_BASED_COMPENSATION_TAGS);
    let mut diluted_shares = extract_share_metric(body, &DILUTED_SHARES_TAGS);
    let diluted_share_outliers = diluted_shares
        .as_mut()
        .map(remove_isolated_scale_outliers)
        .unwrap_or_default();
    let free_cash_flow_proxy =
        free_cash_flow_proxy(operating_cash_flow.as_ref(), capital_expenditure.as_ref());
    let free_cash_flow_per_share_proxy =
        per_share_proxy(free_cash_flow_proxy.as_ref(), diluted_shares.as_ref());
    let metrics = [
        ("revenue", revenue.as_ref()),
        ("gross profit", gross_profit.as_ref()),
        ("cost of revenue", cost_of_revenue.as_ref()),
        ("operating income", operating_income.as_ref()),
        ("net income", net_income.as_ref()),
        ("selling and marketing expense", selling_marketing.as_ref()),
        ("operating cash flow", operating_cash_flow.as_ref()),
        ("capital expenditure", capital_expenditure.as_ref()),
        ("free-cash-flow proxy", free_cash_flow_proxy.as_ref()),
        (
            "share-based compensation",
            share_based_compensation.as_ref(),
        ),
        ("diluted weighted-average shares", diluted_shares.as_ref()),
        (
            "free-cash-flow proxy per diluted share",
            free_cash_flow_per_share_proxy.as_ref(),
        ),
    ];
    let mut years = metrics
        .iter()
        .flat_map(|(_, metric)| metric.iter().flat_map(|metric| metric.values.keys()))
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .rev()
        .take(requested_years.clamp(2, 10))
        .collect::<Vec<_>>();
    if years.len() < 2 {
        return None;
    }
    years.sort_unstable();

    let units = metrics
        .iter()
        .filter_map(|(label, metric)| metric.map(|metric| format!("{label} {}", metric.unit)))
        .collect::<Vec<_>>()
        .join(", ");
    let mut rows = Vec::with_capacity(years.len());
    for year in &years {
        let values = metrics
            .iter()
            .filter_map(|(label, metric)| {
                let metric = metric.as_ref()?;
                metric
                    .values
                    .get(year)
                    .map(|fact| format!("{label} {}", format_metric_value(metric, fact.value)))
            })
            .collect::<Vec<_>>()
            .join("; ");
        rows.push(format!("{year}: {values}."));
    }
    let latest_filed = metrics
        .iter()
        .flat_map(|(_, metric)| metric.iter().flat_map(|metric| metric.values.values()))
        .map(|fact| fact.filed.as_str())
        .max()
        .unwrap_or("unknown");
    let first_year = years.first()?;
    let last_year = years.last()?;
    let entity_name = body
        .get("entityName")
        .and_then(Value::as_str)
        .unwrap_or("Company");
    let proxy_note = free_cash_flow_proxy.as_ref().map_or("", |_| {
        " Free-cash-flow proxy equals operating cash flow minus total capital expenditure. It is not owner earnings because SEC facts do not separate maintenance from growth capital expenditure."
    });
    let quality_note = (!diluted_share_outliers.is_empty()).then(|| {
        format!(
            " Excluded diluted weighted-average shares for {} as an isolated scale outlier versus both adjacent annual facts.",
            diluted_share_outliers
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    });
    Some(ConversationResearchSource {
        title: format!(
            "{entity_name} SEC XBRL annual facts ({first_year}-{last_year})"
        ),
        url: url.to_string(),
        snippet: format!(
            "Official SEC Company Facts annual series, latest filing date {latest_filed}. Reported units: {units}. {} Values select the latest filed annual fact for each calendar-year frame; figures are rounded to three decimals for display.{proxy_note}{}",
            rows.join(" "),
            quality_note.as_deref().unwrap_or_default(),
        ),
        source_tier: "primary".to_string(),
    })
}

struct AnnualMetric {
    unit: MetricUnit,
    values: BTreeMap<i32, AnnualFact>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MetricUnit {
    Currency(String),
    Shares,
    CurrencyPerShare(String),
}

impl fmt::Display for MetricUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Currency(currency) => formatter.write_str(currency),
            Self::Shares => formatter.write_str("shares"),
            Self::CurrencyPerShare(currency) => write!(formatter, "{currency}/share"),
        }
    }
}

struct AnnualFact {
    value: f64,
    filed: String,
}

fn extract_currency_metric(body: &Value, tags: &[(&str, &str)]) -> Option<AnnualMetric> {
    extract_metric(body, tags, currency_unit)
}

fn extract_share_metric(body: &Value, tags: &[(&str, &str)]) -> Option<AnnualMetric> {
    extract_metric(body, tags, share_unit)
}

fn extract_metric(
    body: &Value,
    tags: &[(&str, &str)],
    parse_unit: fn(&str) -> Option<MetricUnit>,
) -> Option<AnnualMetric> {
    let facts = body.get("facts")?;
    let mut best = None::<AnnualMetric>;
    for (namespace, tag) in tags {
        let Some(units) = facts
            .get(*namespace)
            .and_then(|namespace| namespace.get(*tag))
            .and_then(|fact| fact.get("units"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        for (unit, entries) in units {
            let Some(unit) = parse_unit(unit) else {
                continue;
            };
            let values = parse_annual_values(entries);
            if values.len() > best.as_ref().map_or(0, |metric| metric.values.len()) {
                best = Some(AnnualMetric { unit, values });
            }
        }
    }
    best.filter(|metric| !metric.values.is_empty())
}

fn free_cash_flow_proxy(
    operating_cash_flow: Option<&AnnualMetric>,
    capital_expenditure: Option<&AnnualMetric>,
) -> Option<AnnualMetric> {
    let operating_cash_flow = operating_cash_flow?;
    let capital_expenditure = capital_expenditure?;
    if operating_cash_flow.unit != capital_expenditure.unit {
        return None;
    }
    let values = operating_cash_flow
        .values
        .iter()
        .filter_map(|(year, cash_flow)| {
            let capex = capital_expenditure.values.get(year)?;
            Some((
                *year,
                AnnualFact {
                    value: cash_flow.value - capex.value.abs(),
                    filed: latest_filed(&cash_flow.filed, &capex.filed).to_string(),
                },
            ))
        })
        .collect::<BTreeMap<_, _>>();
    (!values.is_empty()).then(|| AnnualMetric {
        unit: operating_cash_flow.unit.clone(),
        values,
    })
}

fn per_share_proxy(
    cash_flow: Option<&AnnualMetric>,
    diluted_shares: Option<&AnnualMetric>,
) -> Option<AnnualMetric> {
    let cash_flow = cash_flow?;
    let diluted_shares = diluted_shares?;
    let MetricUnit::Currency(currency) = &cash_flow.unit else {
        return None;
    };
    let values = cash_flow
        .values
        .iter()
        .filter_map(|(year, cash_flow)| {
            let shares = diluted_shares.values.get(year)?;
            (shares.value != 0.0).then(|| {
                (
                    *year,
                    AnnualFact {
                        value: cash_flow.value / shares.value,
                        filed: latest_filed(&cash_flow.filed, &shares.filed).to_string(),
                    },
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    (!values.is_empty()).then(|| AnnualMetric {
        unit: MetricUnit::CurrencyPerShare(currency.clone()),
        values,
    })
}

fn remove_isolated_scale_outliers(metric: &mut AnnualMetric) -> Vec<i32> {
    let years = metric.values.keys().copied().collect::<Vec<_>>();
    let mut outliers = Vec::new();
    for window in years.windows(3) {
        let [left_year, year, right_year] = window else {
            continue;
        };
        let [Some(left), Some(current), Some(right)] = [
            metric.values.get(left_year),
            metric.values.get(year),
            metric.values.get(right_year),
        ] else {
            continue;
        };
        if scale_ratio(current.value, left.value) >= ISOLATED_SCALE_OUTLIER_RATIO
            && scale_ratio(current.value, right.value) >= ISOLATED_SCALE_OUTLIER_RATIO
        {
            outliers.push(*year);
        }
    }
    for year in &outliers {
        metric.values.remove(year);
    }
    outliers
}

fn scale_ratio(left: f64, right: f64) -> f64 {
    let smaller = left.abs().min(right.abs());
    let larger = left.abs().max(right.abs());
    if smaller == 0.0 {
        f64::INFINITY
    } else {
        larger / smaller
    }
}

fn latest_filed<'a>(left: &'a str, right: &'a str) -> &'a str {
    if left >= right {
        left
    } else {
        right
    }
}

fn parse_annual_values(entries: &Value) -> BTreeMap<i32, AnnualFact> {
    let mut values = BTreeMap::new();
    for entry in entries.as_array().into_iter().flatten() {
        let form = entry
            .get("form")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(
            form,
            "10-K" | "10-K/A" | "20-F" | "20-F/A" | "40-F" | "40-F/A"
        ) || entry
            .get("fp")
            .and_then(Value::as_str)
            .is_some_and(|period| period != "FY")
        {
            continue;
        }
        let Some(year) = annual_year(entry) else {
            continue;
        };
        let Some(value) = entry.get("val").and_then(number_value) else {
            continue;
        };
        let filed = entry
            .get("filed")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let should_replace = values
            .get(&year)
            .is_none_or(|current: &AnnualFact| filed >= current.filed);
        if should_replace {
            values.insert(year, AnnualFact { value, filed });
        }
    }
    values
}

fn annual_year(entry: &Value) -> Option<i32> {
    if let Some(frame) = entry.get("frame").and_then(Value::as_str) {
        if frame.len() == 6 && frame.starts_with("CY") {
            return frame[2..].parse().ok();
        }
    }
    let start = NaiveDate::parse_from_str(entry.get("start")?.as_str()?, "%Y-%m-%d").ok()?;
    let end = NaiveDate::parse_from_str(entry.get("end")?.as_str()?, "%Y-%m-%d").ok()?;
    (300..=380)
        .contains(&(end - start).num_days())
        .then_some(end.year())
}

fn number_value(value: &Value) -> Option<f64> {
    value
        .as_i64()
        .map(|value| value as f64)
        .or_else(|| value.as_u64().map(|value| value as f64))
        .or_else(|| value.as_f64())
}

fn currency_unit(unit: &str) -> Option<MetricUnit> {
    (unit.len() == 3 && unit.bytes().all(|byte| byte.is_ascii_uppercase()))
        .then(|| MetricUnit::Currency(unit.to_string()))
}

fn share_unit(unit: &str) -> Option<MetricUnit> {
    unit.eq_ignore_ascii_case("shares")
        .then_some(MetricUnit::Shares)
}

fn format_metric_value(metric: &AnnualMetric, value: f64) -> String {
    if matches!(metric.unit, MetricUnit::CurrencyPerShare(_)) {
        format!("{value:.3}")
    } else {
        format_value(value)
    }
}

fn format_value(value: f64) -> String {
    if value.abs() >= 1_000_000_000.0 {
        format!("{:.3} billion", value / 1_000_000_000.0)
    } else if value.abs() >= 1_000_000.0 {
        format!("{:.3} million", value / 1_000_000.0)
    } else {
        format!("{value:.0}")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{company_facts_url, source_from_company_facts};

    fn annual_facts(values: &[(i32, i64)]) -> Value {
        Value::Array(
            values
                .iter()
                .map(|(year, value)| {
                    json!({
                        "form": "20-F",
                        "fp": "FY",
                        "frame": format!("CY{year}"),
                        "val": value,
                        "filed": format!("{}-04-30", year + 1)
                    })
                })
                .collect(),
        )
    }

    #[test]
    fn derives_company_facts_url_from_an_sec_filing() {
        let url = company_facts_url(
            "https://www.sec.gov/Archives/edgar/data/1737806/0001/filing-index.htm",
        );

        assert_eq!(
            url.as_deref(),
            Some("https://data.sec.gov/api/xbrl/companyfacts/CIK0001737806.json")
        );
    }

    #[test]
    fn builds_a_five_year_financial_series_from_sec_xbrl_facts() {
        let years = [
            (2021, 100_000_000_000),
            (2022, 130_000_000_000),
            (2023, 247_000_000_000),
            (2024, 393_000_000_000),
            (2025, 431_000_000_000),
        ];
        let body = json!({
            "entityName": "PDD Holdings Inc.",
            "facts": { "us-gaap": {
                "RevenueFromContractWithCustomerExcludingAssessedTax": {
                    "units": { "CNY": annual_facts(&years) }
                },
                "NetIncomeLoss": {
                    "units": { "CNY": annual_facts(&[
                        (2021, 7_000_000_000), (2022, 31_000_000_000),
                        (2023, 60_000_000_000), (2024, 112_000_000_000),
                        (2025, 97_000_000_000)
                    ]) }
                },
                "GrossProfit": {
                    "units": { "CNY": annual_facts(&[
                        (2021, 66_000_000_000), (2022, 99_000_000_000),
                        (2023, 155_000_000_000), (2024, 242_000_000_000),
                        (2025, 257_000_000_000)
                    ]) }
                },
                "CostOfRevenue": {
                    "units": { "CNY": annual_facts(&[
                        (2021, 34_000_000_000), (2022, 31_000_000_000),
                        (2023, 93_000_000_000), (2024, 152_000_000_000),
                        (2025, 175_000_000_000)
                    ]) }
                },
                "OperatingIncomeLoss": {
                    "units": { "CNY": annual_facts(&[
                        (2021, 7_000_000_000), (2022, 37_000_000_000),
                        (2023, 59_000_000_000), (2024, 108_000_000_000),
                        (2025, 95_000_000_000)
                    ]) }
                },
                "SellingAndMarketingExpense": {
                    "units": { "CNY": annual_facts(&[
                        (2021, 45_000_000_000), (2022, 54_000_000_000),
                        (2023, 82_000_000_000), (2024, 112_000_000_000),
                        (2025, 125_000_000_000)
                    ]) }
                },
                "NetCashProvidedByUsedInOperatingActivities": {
                    "units": { "CNY": annual_facts(&[
                        (2021, 28_000_000_000), (2022, 48_000_000_000),
                        (2023, 94_000_000_000), (2024, 121_000_000_000),
                        (2025, 106_000_000_000)
                    ]) }
                },
                "PaymentsToAcquirePropertyPlantAndEquipment": {
                    "units": { "CNY": annual_facts(&[
                        (2021, 2_000_000_000), (2022, 3_000_000_000),
                        (2023, 5_000_000_000), (2024, 7_000_000_000),
                        (2025, 8_000_000_000)
                    ]) }
                },
                "ShareBasedCompensation": {
                    "units": { "CNY": annual_facts(&[
                        (2021, 9_000_000_000), (2022, 10_000_000_000),
                        (2023, 12_000_000_000), (2024, 14_000_000_000),
                        (2025, 15_000_000_000)
                    ]) }
                },
                "WeightedAverageNumberOfDilutedSharesOutstanding": {
                    "units": { "shares": annual_facts(&[
                        (2021, 5_000_000_000), (2022, 5_100_000),
                        (2023, 5_200_000_000), (2024, 5_300_000_000),
                        (2025, 5_400_000_000)
                    ]) }
                }
            }}
        });

        let source = source_from_company_facts(
            &body,
            "https://data.sec.gov/api/xbrl/companyfacts/CIK0001737806.json",
            5,
        )
        .expect("company facts source");

        assert!(source.title.contains("2021-2025"));
        assert!(source.snippet.contains("2021: revenue 100.000 billion"));
        assert!(source.snippet.contains("2025: revenue 431.000 billion"));
        assert!(source.snippet.contains("net income 97.000 billion"));
        assert!(source.snippet.contains("gross profit 257.000 billion"));
        assert!(source.snippet.contains("cost of revenue 175.000 billion"));
        assert!(source.snippet.contains("operating income 95.000 billion"));
        assert!(source
            .snippet
            .contains("selling and marketing expense 125.000 billion"));
        assert!(source
            .snippet
            .contains("operating cash flow 106.000 billion"));
        assert!(source.snippet.contains("capital expenditure 8.000 billion"));
        assert!(source
            .snippet
            .contains("share-based compensation 15.000 billion"));
        assert!(source
            .snippet
            .contains("diluted weighted-average shares 5.400 billion"));
        assert!(source
            .snippet
            .contains("free-cash-flow proxy 98.000 billion"));
        assert!(source
            .snippet
            .contains("free-cash-flow proxy per diluted share 18.148"));
        assert!(!source
            .snippet
            .contains("2022: diluted weighted-average shares 5.100 million"));
        assert!(source.snippet.contains(
            "Excluded diluted weighted-average shares for 2022 as an isolated scale outlier"
        ));
        assert!(source.snippet.contains(
            "Free-cash-flow proxy equals operating cash flow minus total capital expenditure"
        ));
        assert!(source.snippet.contains(
            "It is not owner earnings because SEC facts do not separate maintenance from growth capital expenditure"
        ));
        assert_eq!(source.source_tier, "primary");
    }
}
