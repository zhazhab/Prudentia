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

use super::{CompanyResearchRequest, ResearchError};

const CLIENT_USER_AGENT: &str = "Mozilla/5.0 (compatible; Prudentia/0.1; local investment memo)";
const SEC_USER_AGENT: &str = "Prudentia prudentia-local@example.com";

pub(super) async fn official_filings(
    client: &Client,
    request: &CompanyResearchRequest,
) -> Result<Vec<ConversationResearchSource>, ResearchError> {
    let symbol = request.base_symbol();
    let response = client
        .get("https://www.sec.gov/cgi-bin/browse-edgar")
        .header(USER_AGENT, SEC_USER_AGENT)
        .query(&[
            ("action", "getcompany"),
            ("CIK", symbol.as_str()),
            ("owner", "exclude"),
            ("count", "10"),
            ("output", "atom"),
        ])
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let candidates = parse_sec_filings(&response, &request.company_name)?;
    let mut sources = Vec::new();
    for candidate in candidates {
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
    request: &CompanyResearchRequest,
) -> Result<Vec<ConversationResearchSource>, ResearchError> {
    let symbol = request.base_symbol();
    let response: YahooSearchResponse = client
        .get("https://query1.finance.yahoo.com/v1/finance/search")
        .header(USER_AGENT, CLIENT_USER_AGENT)
        .query(&[
            ("q", symbol.as_str()),
            ("quotesCount", "1"),
            ("newsCount", "12"),
        ])
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(parse_yahoo_news(response, request))
}

pub(super) async fn community_discussions(
    client: &Client,
    request: &CompanyResearchRequest,
) -> Result<Vec<ConversationResearchSource>, ResearchError> {
    let symbol = request.base_symbol();
    let mut url = Url::parse("https://www.tradingview.com/").expect("valid TradingView URL");
    url.path_segments_mut()
        .expect("TradingView URL supports path segments")
        .push("symbols")
        .push(&symbol)
        .push("ideas")
        .push("");
    let response = client
        .get(url)
        .header(USER_AGENT, CLIENT_USER_AGENT)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(parse_tradingview_ideas(&response, request))
}

fn parse_sec_filings(
    body: &str,
    company_name: &str,
) -> Result<Vec<ConversationResearchSource>, ResearchError> {
    let feed: SecFeed =
        quick_xml::de::from_str(body).map_err(|error| ResearchError::Payload(error.to_string()))?;
    Ok(feed
        .entries
        .into_iter()
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
        .take(3)
        .collect())
}

fn parse_yahoo_news(
    response: YahooSearchResponse,
    request: &CompanyResearchRequest,
) -> Vec<ConversationResearchSource> {
    let symbol = request.base_symbol();
    response
        .news
        .into_iter()
        .filter(|item| {
            item.related_tickers
                .iter()
                .any(|ticker| ticker.eq_ignore_ascii_case(&symbol))
                && relevant_investment_text(&item.title, &symbol, &request.company_name)
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
    let index = client
        .get(&source.url)
        .header(USER_AGENT, SEC_USER_AGENT)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let Some(document_url) = best_sec_document_url(&source.url, &index) else {
        return Ok(None);
    };
    let response = client
        .get(&document_url)
        .header(USER_AGENT, SEC_USER_AGENT)
        .header(RANGE, "bytes=0-524287")
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?;
    let body = read_text_prefix(response, 512 * 1024).await?;
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
            let url = allowed_url(url.as_str(), &["sec.gov"])?;
            Some((priority, url))
        })
        .min_by_key(|(priority, _)| *priority)
        .map(|(_, url)| url)
}

fn official_document_excerpt(body: &str) -> String {
    let document = Html::parse_document(body);
    truncate_chars(
        &document
            .root_element()
            .text()
            .flat_map(str::split_whitespace)
            .collect::<Vec<_>>()
            .join(" "),
        3_000,
    )
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

fn parse_tradingview_ideas(
    body: &str,
    request: &CompanyResearchRequest,
) -> Vec<ConversationResearchSource> {
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
    let symbol = request.base_symbol();
    ideas
        .into_iter()
        .filter(|idea| {
            idea.is_hot
                && idea.likes_count + idea.comments_count > 0
                && idea.symbol.short_name.eq_ignore_ascii_case(&symbol)
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

fn relevant_investment_text(value: &str, symbol: &str, company_name: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    let company_match =
        company_name.len() >= 4 && normalized.contains(&company_name.to_ascii_lowercase());
    let symbol_match = contains_token(&normalized, &symbol.to_ascii_lowercase());
    let investment_context = [
        "stock",
        "share",
        "invest",
        "buy",
        "sell",
        "opportunity",
        "analyst",
        "analysis",
        "earning",
        "valuation",
        "market",
        "trading",
        "ticker",
        "bull",
        "bear",
        "portfolio",
        "sentiment",
        "revenue",
        "margin",
    ]
    .iter()
    .any(|term| normalized.contains(term));
    company_match || (symbol_match && investment_context)
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
    use crate::conversation::research::{CompanyResearchIntent, CompanyResearchRequest};

    fn pdd_request() -> CompanyResearchRequest {
        CompanyResearchRequest {
            company_name: "PDD Holdings".to_string(),
            symbol: "PDD".to_string(),
            intent: CompanyResearchIntent::Earnings,
        }
    }

    #[test]
    fn parses_official_sec_filings() {
        let xml = r#"<feed><entry><content><accession-number>0001</accession-number><filing-date>2026-05-28</filing-date><filing-href>https://www.sec.gov/Archives/filing-index.htm</filing-href><filing-type>6-K</filing-type><form-name>Report of foreign issuer</form-name><size>212 KB</size></content></entry></feed>"#;

        let sources = parse_sec_filings(xml, "PDD Holdings").expect("SEC feed");

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source_tier, "primary");
        assert!(sources[0].snippet.contains("2026-05-28"));
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
    fn yahoo_news_requires_the_requested_ticker() {
        let response: YahooSearchResponse = serde_json::from_str(
            r#"{"news":[{"title":"PDD analysis","publisher":"Example","link":"https://finance.yahoo.com/article/pdd","providerPublishTime":1780000000,"relatedTickers":["PDD"]},{"title":"Other","publisher":"Example","link":"https://finance.yahoo.com/article/other","providerPublishTime":1780000000,"relatedTickers":["BABA"]}]}"#,
        )
        .expect("Yahoo fixture");

        let sources = parse_yahoo_news(response, &pdd_request());

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].title, "PDD analysis");
    }

    #[test]
    fn tradingview_results_require_hot_ideas_with_engagement() {
        let html = r#"<html><script type="application/prs.init-data+json">{"payload":{"ideas":[{"name":"PDD valuation debate","description":"Margins are the key disagreement.","created_at":"2026-05-29T00:00:00Z","chart_url":"https://www.tradingview.com/chart/PDD/example/","is_hot":true,"comments_count":9,"likes_count":4,"symbol":{"short_name":"PDD"}},{"name":"No engagement","description":"Ignore this.","created_at":"2026-05-29T00:00:00Z","chart_url":"https://www.tradingview.com/chart/PDD/quiet/","is_hot":true,"comments_count":0,"likes_count":0,"symbol":{"short_name":"PDD"}}]}}</script></html>"#;

        let sources = parse_tradingview_ideas(html, &pdd_request());

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source_tier, "community");
        assert!(sources[0].snippet.contains("4 likes and 9 comments"));
        assert!(sources[0]
            .snippet
            .contains("Margins are the key disagreement"));
    }
}
