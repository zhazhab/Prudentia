use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use reqwest::{
    header::{RANGE, USER_AGENT},
    Client, Url,
};
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::Value;

use crate::ai::ConversationResearchSource;

use super::{ResearchError, ResearchPlan};

mod filing_excerpt;
mod http_retry;
mod sec_company_facts;

use filing_excerpt::official_document_excerpt;

const CLIENT_USER_AGENT: &str = "Mozilla/5.0 (compatible; Prudentia/0.1; local investment memo)";
const SEC_USER_AGENT: &str = "Prudentia prudentia-local@example.com";

pub(super) async fn official_filings(
    client: &Client,
    plan: &ResearchPlan,
) -> Result<Vec<ConversationResearchSource>, ResearchError> {
    let symbol = plan.base_symbol();
    let annual_history_years = plan.annual_history_years();
    let filing_count = if annual_history_years.is_some() {
        "40"
    } else {
        "10"
    };
    let response = http_retry::text(
        client
            .get("https://www.sec.gov/cgi-bin/browse-edgar")
            .header(USER_AGENT, SEC_USER_AGENT)
            .query(&[
                ("action", "getcompany"),
                ("CIK", symbol.as_str()),
                ("owner", "exclude"),
                ("count", filing_count),
                ("output", "atom"),
            ])
            .timeout(std::time::Duration::from_secs(20)),
    )
    .await?;
    let candidates = parse_sec_filings(&response, plan.company_name(), annual_history_years)?;
    let mut sources = Vec::new();
    if let (Some(years), Some(candidate)) = (annual_history_years, candidates.first()) {
        match fetch_company_facts(client, &candidate.url, years).await {
            Ok(Some(source)) => sources.push(source),
            Ok(None) => tracing::warn!(
                years,
                "SEC Company Facts did not contain a usable annual financial series"
            ),
            Err(error) => tracing::warn!(%error, "SEC Company Facts retrieval failed"),
        }
    }
    let document_limit = if sources.is_empty() { 3 } else { 1 };
    for candidate in candidates.into_iter().take(document_limit) {
        match enrich_sec_filing(client, candidate).await {
            Ok(Some(source)) => sources.push(source),
            Ok(None) => tracing::warn!("SEC filing contained no readable primary document"),
            Err(error) => tracing::warn!(%error, "SEC filing document retrieval failed"),
        }
    }
    Ok(sources)
}

pub(super) async fn independent_news(
    client: &Client,
    plan: &ResearchPlan,
) -> Result<Vec<ConversationResearchSource>, ResearchError> {
    let symbol = plan.base_symbol();
    let body = http_retry::text(
        client
            .get("https://query1.finance.yahoo.com/v1/finance/search")
            .header(USER_AGENT, CLIENT_USER_AGENT)
            .query(&[
                ("q", symbol.as_str()),
                ("quotesCount", "1"),
                ("newsCount", "12"),
            ])
            .timeout(std::time::Duration::from_secs(20)),
    )
    .await?;
    let response: YahooSearchResponse =
        serde_json::from_str(&body).map_err(|error| ResearchError::Payload(error.to_string()))?;
    Ok(parse_yahoo_news(response, plan))
}

pub(super) async fn community_discussions(
    client: &Client,
    plan: &ResearchPlan,
) -> Result<Vec<ConversationResearchSource>, ResearchError> {
    let symbol = plan.base_symbol();
    let mut url = Url::parse("https://www.tradingview.com/").expect("valid TradingView URL");
    url.path_segments_mut()
        .expect("TradingView URL supports path segments")
        .push("symbols")
        .push(&symbol)
        .push("ideas")
        .push("");
    let response = http_retry::text(
        client
            .get(url)
            .header(USER_AGENT, CLIENT_USER_AGENT)
            .timeout(std::time::Duration::from_secs(20)),
    )
    .await?;
    Ok(parse_tradingview_ideas(&response, plan))
}

fn parse_sec_filings(
    body: &str,
    company_name: &str,
    annual_history_years: Option<usize>,
) -> Result<Vec<ConversationResearchSource>, ResearchError> {
    let feed: SecFeed =
        quick_xml::de::from_str(body).map_err(|error| ResearchError::Payload(error.to_string()))?;
    let limit = annual_history_years.unwrap_or(3).clamp(1, 10);
    Ok(feed
        .entries
        .into_iter()
        .filter(|entry| {
            annual_history_years.is_none() || is_annual_form(&entry.content.filing_type)
        })
        .filter_map(|entry| {
            let url = allowed_url(&entry.content.filing_href, &["sec.gov"])?;
            Some(ConversationResearchSource {
                title: format!(
                    "{company_name} SEC {}: {}",
                    entry.content.filing_type, entry.content.form_name
                ),
                url,
                snippet: format!(
                    "Official SEC filing dated {}; accession {}; size {}.",
                    entry.content.filing_date, entry.content.accession_number, entry.content.size
                ),
                source_tier: "primary".to_string(),
            })
        })
        .take(limit)
        .collect())
}

fn is_annual_form(form: &str) -> bool {
    matches!(
        form,
        "10-K" | "10-K/A" | "20-F" | "20-F/A" | "40-F" | "40-F/A"
    )
}

async fn fetch_company_facts(
    client: &Client,
    filing_url: &str,
    years: usize,
) -> Result<Option<ConversationResearchSource>, ResearchError> {
    let Some(url) = sec_company_facts::company_facts_url(filing_url) else {
        return Ok(None);
    };
    let response = http_retry::text(
        client
            .get(&url)
            .header(USER_AGENT, SEC_USER_AGENT)
            .timeout(std::time::Duration::from_secs(20)),
    )
    .await?;
    let body: Value = serde_json::from_str(&response)
        .map_err(|error| ResearchError::Payload(error.to_string()))?;
    Ok(sec_company_facts::source_from_company_facts(
        &body, &url, years,
    ))
}

fn parse_yahoo_news(
    response: YahooSearchResponse,
    plan: &ResearchPlan,
) -> Vec<ConversationResearchSource> {
    let symbol = plan.base_symbol();
    response
        .news
        .into_iter()
        .filter(|item| {
            item.related_tickers
                .iter()
                .any(|ticker| ticker.eq_ignore_ascii_case(&symbol))
                && relevant_company_operating_text(&item.title, &symbol, plan.company_name())
        })
        .filter_map(|item| {
            let url = allowed_url(&item.link, &["finance.yahoo.com"])?;
            let published = DateTime::<Utc>::from_timestamp(item.provider_publish_time, 0)
                .map(|time| time.date_naive().to_string())
                .unwrap_or_else(|| "unknown date".to_string());
            Some(ConversationResearchSource {
                title: item.title,
                url,
                snippet: format!(
                    "Published by {} on {published}; associated with ticker {symbol}.",
                    item.publisher
                ),
                source_tier: "secondary".to_string(),
            })
        })
        .take(3)
        .collect()
}

async fn enrich_sec_filing(
    client: &Client,
    mut source: ConversationResearchSource,
) -> Result<Option<ConversationResearchSource>, ResearchError> {
    let index = http_retry::text(
        client
            .get(&source.url)
            .header(USER_AGENT, SEC_USER_AGENT)
            .timeout(std::time::Duration::from_secs(20)),
    )
    .await?;
    let Some(document_url) = best_sec_document_url(&source.url, &index) else {
        return Ok(None);
    };
    let response = http_retry::response(
        client
            .get(&document_url)
            .header(USER_AGENT, SEC_USER_AGENT)
            .header(RANGE, "bytes=0-1572863")
            .timeout(std::time::Duration::from_secs(20)),
    )
    .await?;
    let body = read_text_prefix(response, 1_536 * 1024).await?;
    let excerpt = official_document_excerpt(&body);
    if excerpt.is_empty() {
        return Ok(None);
    }
    source.url = document_url;
    source.snippet.push_str(" Filing excerpt: ");
    source.snippet.push_str(&excerpt);
    Ok(Some(source))
}

fn best_sec_document_url(index_url: &str, body: &str) -> Option<String> {
    let document = Html::parse_document(body);
    let row_selector = Selector::parse("table.tableFile tr").expect("valid SEC row selector");
    let cell_selector = Selector::parse("td").expect("valid SEC cell selector");
    let link_selector = Selector::parse("a[href]").expect("valid SEC link selector");
    let base = Url::parse(index_url).ok()?;
    document
        .select(&row_selector)
        .filter_map(|row| {
            let cells = row
                .select(&cell_selector)
                .map(|cell| normalized_text(&cell.text().collect::<String>()))
                .collect::<Vec<_>>();
            let filing_type = cells.get(3)?.to_ascii_uppercase();
            let priority = match filing_type.as_str() {
                "EX-99.1" => 0,
                "10-Q" | "10-K" | "20-F" => 1,
                "6-K" | "8-K" => 2,
                _ => return None,
            };
            let href = row.select(&link_selector).next()?.value().attr("href")?;
            let url = base.join(href).ok()?;
            let url = direct_sec_document_url(url)?;
            let url = allowed_url(url.as_str(), &["sec.gov"])?;
            Some((priority, url))
        })
        .min_by_key(|(priority, _)| *priority)
        .map(|(_, url)| url)
}

fn direct_sec_document_url(url: Url) -> Option<Url> {
    if url.path() != "/ix" {
        return Some(url);
    }
    let document_path = url
        .query_pairs()
        .find_map(|(key, value)| (key == "doc").then(|| value.into_owned()))?;
    Url::parse("https://www.sec.gov")
        .ok()?
        .join(&document_path)
        .ok()
}

async fn read_text_prefix(
    response: reqwest::Response,
    limit: usize,
) -> Result<String, ResearchError> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let remaining = limit.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if bytes.len() >= limit {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn parse_tradingview_ideas(body: &str, plan: &ResearchPlan) -> Vec<ConversationResearchSource> {
    let document = Html::parse_document(body);
    let selector = Selector::parse(r#"script[type="application/prs.init-data+json"]"#)
        .expect("valid TradingView data selector");
    let mut ideas = Vec::new();
    for script in document.select(&selector) {
        let raw = script.text().collect::<String>();
        if let Ok(value) = serde_json::from_str::<Value>(&raw) {
            collect_tradingview_ideas(&value, &mut ideas);
        }
    }
    let symbol = plan.base_symbol();
    ideas
        .into_iter()
        .filter(|idea| {
            idea.is_hot
                && idea.likes_count + idea.comments_count > 0
                && idea.symbol.short_name.eq_ignore_ascii_case(&symbol)
                && contains_company_operating_term(&format!("{} {}", idea.name, idea.description))
                && !contains_capital_market_term(&format!("{} {}", idea.name, idea.description))
        })
        .filter_map(|idea| {
            let url = allowed_url(&idea.chart_url, &["tradingview.com"])?;
            let excerpt = truncate_chars(&normalized_text(&idea.description), 800);
            Some(ConversationResearchSource {
                title: idea.name,
                url,
                snippet: format!(
                    "TradingView hot idea for {symbol}; {} likes and {} comments; published {}. {excerpt}",
                    idea.likes_count, idea.comments_count, idea.created_at
                ),
                source_tier: "community".to_string(),
            })
        })
        .take(3)
        .collect()
}

fn collect_tradingview_ideas(value: &Value, ideas: &mut Vec<TradingViewIdea>) {
    match value {
        Value::Object(object)
            if object.contains_key("chart_url") && object.contains_key("is_hot") =>
        {
            if let Ok(idea) = serde_json::from_value::<TradingViewIdea>(value.clone()) {
                ideas.push(idea);
            }
        }
        Value::Object(object) => {
            for child in object.values() {
                collect_tradingview_ideas(child, ideas);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_tradingview_ideas(child, ideas);
            }
        }
        _ => {}
    }
}

fn allowed_url(value: &str, domains: &[&str]) -> Option<String> {
    let mut url = Url::parse(value).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let allowed = domains
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")));
    if url.scheme() != "https"
        || !allowed
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|port| port != 443)
    {
        return None;
    }
    url.set_fragment(None);
    Some(url.to_string())
}

fn normalized_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn relevant_company_operating_text(value: &str, symbol: &str, company_name: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    let company_match =
        company_name.len() >= 4 && normalized.contains(&company_name.to_ascii_lowercase());
    let symbol_match = contains_token(&normalized, &symbol.to_ascii_lowercase());
    (company_match || symbol_match)
        && contains_company_operating_term(&normalized)
        && !contains_capital_market_term(&normalized)
}

fn contains_company_operating_term(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    [
        "business",
        "operation",
        "revenue",
        "sales",
        "margin",
        "profit",
        "earning",
        "cash flow",
        "customer",
        "merchant",
        "supplier",
        "product",
        "service",
        "competition",
        "competitor",
        "market share",
        "growth",
        "cost",
        "regulation",
        "strategy",
        "guidance",
        "result",
        "收入",
        "利润",
        "现金流",
        "客户",
        "商家",
        "竞争",
        "成本",
        "业务",
    ]
    .iter()
    .any(|term| normalized.contains(term))
}

fn contains_capital_market_term(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    [
        "share price",
        "price target",
        "market capitalization",
        "technical analysis",
    ]
    .iter()
    .any(|term| normalized.contains(term))
        || [
            "stock",
            "shares",
            "valuation",
            "undervalued",
            "overvalued",
            "buy",
            "sell",
            "long",
            "short",
            "chart",
            "ticker",
            "portfolio",
            "bull",
            "bear",
            "investing",
        ]
        .iter()
        .any(|term| contains_token(&normalized, term))
}

fn contains_token(value: &str, token: &str) -> bool {
    value.match_indices(token).any(|(index, _)| {
        let before = value[..index].chars().next_back();
        let after = value[index + token.len()..].chars().next();
        before.is_none_or(|character| !character.is_ascii_alphanumeric())
            && after.is_none_or(|character| !character.is_ascii_alphanumeric())
    })
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut result = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        result.push_str("...");
    }
    result
}

#[derive(Deserialize)]
struct SecFeed {
    #[serde(rename = "entry", default)]
    entries: Vec<SecEntry>,
}

#[derive(Deserialize)]
struct SecEntry {
    content: SecFiling,
}

#[derive(Deserialize)]
struct SecFiling {
    #[serde(rename = "accession-number")]
    accession_number: String,
    #[serde(rename = "filing-date")]
    filing_date: String,
    #[serde(rename = "filing-href")]
    filing_href: String,
    #[serde(rename = "filing-type")]
    filing_type: String,
    #[serde(rename = "form-name")]
    form_name: String,
    size: String,
}

#[derive(Deserialize)]
struct YahooSearchResponse {
    #[serde(default)]
    news: Vec<YahooNewsItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct YahooNewsItem {
    title: String,
    publisher: String,
    link: String,
    provider_publish_time: i64,
    #[serde(default)]
    related_tickers: Vec<String>,
}

#[derive(Deserialize)]
struct TradingViewIdea {
    name: String,
    description: String,
    created_at: String,
    chart_url: String,
    is_hot: bool,
    #[serde(default)]
    comments_count: u64,
    #[serde(default)]
    likes_count: u64,
    symbol: TradingViewSymbol,
}

#[derive(Deserialize)]
struct TradingViewSymbol {
    short_name: String,
}

#[cfg(test)]
mod tests {
    use super::{
        best_sec_document_url, official_document_excerpt, parse_sec_filings,
        parse_tradingview_ideas, parse_yahoo_news, YahooSearchResponse,
    };
    use crate::conversation::{
        research::{plan_research, ResearchPlan},
        types::ThreadSubject,
    };

    fn pdd_plan() -> ResearchPlan {
        plan_research(
            "分析 PDD 最新财报",
            &ThreadSubject {
                kind: "company".to_string(),
                subject_key: Some("PDD".to_string()),
                label: Some("PDD Holdings".to_string()),
                confidence: 0.95,
            },
        )
        .expect("research plan")
    }

    #[test]
    fn parses_official_sec_filings() {
        let xml = r#"<feed><entry><content><accession-number>0001</accession-number><filing-date>2026-05-28</filing-date><filing-href>https://www.sec.gov/Archives/filing-index.htm</filing-href><filing-type>6-K</filing-type><form-name>Report of foreign issuer</form-name><size>212 KB</size></content></entry></feed>"#;

        let sources = parse_sec_filings(xml, "PDD Holdings", None).expect("SEC feed");

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source_tier, "primary");
        assert!(sources[0].snippet.contains("2026-05-28"));
    }

    #[test]
    fn historical_sec_selection_uses_annual_filings_only() {
        let xml = r#"<feed>
          <entry><content><accession-number>q1</accession-number><filing-date>2026-05-28</filing-date><filing-href>https://www.sec.gov/Archives/q1-index.htm</filing-href><filing-type>6-K</filing-type><form-name>Quarterly results</form-name><size>1 MB</size></content></entry>
          <entry><content><accession-number>a5</accession-number><filing-date>2026-04-29</filing-date><filing-href>https://www.sec.gov/Archives/a5-index.htm</filing-href><filing-type>20-F</filing-type><form-name>Annual report</form-name><size>10 MB</size></content></entry>
          <entry><content><accession-number>a4</accession-number><filing-date>2025-04-28</filing-date><filing-href>https://www.sec.gov/Archives/a4-index.htm</filing-href><filing-type>20-F</filing-type><form-name>Annual report</form-name><size>10 MB</size></content></entry>
          <entry><content><accession-number>a3</accession-number><filing-date>2024-04-25</filing-date><filing-href>https://www.sec.gov/Archives/a3-index.htm</filing-href><filing-type>20-F</filing-type><form-name>Annual report</form-name><size>10 MB</size></content></entry>
          <entry><content><accession-number>a2</accession-number><filing-date>2023-04-26</filing-date><filing-href>https://www.sec.gov/Archives/a2-index.htm</filing-href><filing-type>20-F</filing-type><form-name>Annual report</form-name><size>10 MB</size></content></entry>
          <entry><content><accession-number>a1</accession-number><filing-date>2022-04-25</filing-date><filing-href>https://www.sec.gov/Archives/a1-index.htm</filing-href><filing-type>20-F</filing-type><form-name>Annual report</form-name><size>10 MB</size></content></entry>
        </feed>"#;

        let sources = parse_sec_filings(xml, "PDD Holdings", Some(5)).expect("SEC feed");

        assert_eq!(sources.len(), 5);
        assert!(sources.iter().all(|source| source.title.contains("20-F")));
        assert!(sources.iter().all(|source| !source.url.contains("q1")));
    }

    #[test]
    fn sec_filing_prefers_earnings_exhibit_and_extracts_readable_facts() {
        let index = r#"<table class="tableFile"><tr><td>1</td><td>FORM 6-K</td><td><a href="main.htm">main.htm</a></td><td>6-K</td></tr><tr><td>2</td><td>EXHIBIT 99.1</td><td><a href="earnings.htm">earnings.htm</a></td><td>EX-99.1</td></tr></table>"#;
        let url = best_sec_document_url(
            "https://www.sec.gov/Archives/example/filing-index.htm",
            index,
        )
        .expect("primary document URL");
        assert_eq!(url, "https://www.sec.gov/Archives/example/earnings.htm");

        let excerpt = official_document_excerpt(
            "<html><body><h1>First Quarter Results</h1><p>Total revenues increased 11%.</p></body></html>",
        );
        assert!(excerpt.contains("Total revenues increased 11%"));
    }

    #[test]
    fn sec_inline_xbrl_links_resolve_to_the_actual_filing_document() {
        let index = r#"<table class="tableFile"><tr><td>1</td><td>FORM 20-F</td><td><a href="/ix?doc=/Archives/edgar/data/1737806/report.htm">report.htm</a></td><td>20-F</td></tr></table>"#;

        let url = best_sec_document_url(
            "https://www.sec.gov/Archives/edgar/data/1737806/filing-index.htm",
            index,
        )
        .expect("direct filing URL");

        assert_eq!(
            url,
            "https://www.sec.gov/Archives/edgar/data/1737806/report.htm"
        );
    }

    #[test]
    fn annual_filing_excerpt_prefers_business_model_evidence() {
        let excerpt = official_document_excerpt(
            r#"<html><body>
            <p>SEC filing boilerplate and table of contents.</p>
            <p>Item 4. Information on the Company</p>
            <p>Item 5. Operating and Financial Review</p>
            <h2>Our Business</h2>
            <p>Consumers use the platform to discover value-for-money products.</p>
            <p>Merchants are the paying customers. The company generates revenue from online marketing services and transaction services.</p>
            <p>The model depends on merchant density, recommendation efficiency, payment volume, and fulfillment costs.</p>
            </body></html>"#,
        );

        assert!(excerpt.starts_with("Our Business"));
        assert!(excerpt.contains("Merchants are the paying customers"));
        assert!(excerpt.contains("generates revenue"));
        assert!(!excerpt.starts_with("SEC filing boilerplate"));
    }

    #[test]
    fn annual_filing_excerpt_includes_competition_and_profit_engine_evidence() {
        let filler = "<p>Unrelated governance disclosure.</p>".repeat(300);
        let filing = format!(
            r#"<html><body>
            <h2>Our Business</h2>
            <p>Consumers discover products while merchants pay for transaction services.</p>
            {filler}
            <h2>Competition</h2>
            <p>We face intense competition for buyers and merchants, who can use multiple platforms with low switching costs.</p>
            {filler}
            <h2>Monetization</h2>
            <p>Revenue comes from online marketing services and transaction services.</p>
            <p>Sales and marketing expenses, merchant subsidies, and fulfillment costs determine operating profitability.</p>
            </body></html>"#
        );
        let excerpt = official_document_excerpt(&filing);

        assert!(excerpt.starts_with("Our Business"));
        assert!(excerpt.contains("intense competition"));
        assert!(excerpt.contains("low switching costs"));
        assert!(excerpt.contains("online marketing services"));
        assert!(excerpt.contains("merchant subsidies"));
    }

    #[test]
    fn yahoo_news_requires_the_requested_ticker() {
        let response: YahooSearchResponse = serde_json::from_str(
            r#"{"news":[{"title":"PDD margin analysis","publisher":"Example","link":"https://finance.yahoo.com/article/pdd","providerPublishTime":1780000000,"relatedTickers":["PDD"]},{"title":"PDD stock looks undervalued despite margin growth","publisher":"Example","link":"https://finance.yahoo.com/article/pdd-stock","providerPublishTime":1780000000,"relatedTickers":["PDD"]},{"title":"Other","publisher":"Example","link":"https://finance.yahoo.com/article/other","providerPublishTime":1780000000,"relatedTickers":["BABA"]}]}"#,
        )
        .expect("Yahoo fixture");

        let sources = parse_yahoo_news(response, &pdd_plan());

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].title, "PDD margin analysis");
    }

    #[test]
    fn tradingview_results_require_hot_ideas_with_engagement() {
        let html = r#"<html><script type="application/prs.init-data+json">{"payload":{"ideas":[{"name":"PDD margin debate","description":"Margins and merchant costs are the key disagreement.","created_at":"2026-05-29T00:00:00Z","chart_url":"https://www.tradingview.com/chart/PDD/example/","is_hot":true,"comments_count":9,"likes_count":4,"symbol":{"short_name":"PDD"}},{"name":"PDD huge long","description":"The price chart is breaking resistance.","created_at":"2026-05-29T00:00:00Z","chart_url":"https://www.tradingview.com/chart/PDD/technical/","is_hot":true,"comments_count":20,"likes_count":40,"symbol":{"short_name":"PDD"}},{"name":"No engagement","description":"Margins are improving.","created_at":"2026-05-29T00:00:00Z","chart_url":"https://www.tradingview.com/chart/PDD/quiet/","is_hot":true,"comments_count":0,"likes_count":0,"symbol":{"short_name":"PDD"}}]}}</script></html>"#;

        let sources = parse_tradingview_ideas(html, &pdd_plan());

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source_tier, "community");
        assert!(sources[0].snippet.contains("4 likes and 9 comments"));
        assert!(sources[0]
            .snippet
            .contains("Margins and merchant costs are the key disagreement"));
        assert!(!sources[0].snippet.contains("price chart"));
    }
}
