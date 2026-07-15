use std::collections::HashSet;

use futures_util::{future::join_all, StreamExt};
use reqwest::{
    header::{CONTENT_TYPE, RANGE},
    redirect::Policy,
    Client, Url,
};
use scraper::{Html, Selector};

use crate::ai::ConversationResearchSource;

const MAX_STORED_SOURCE_SNIPPET_CHARS: usize = 8_000;

pub(super) fn normalize_sources(
    sources: Vec<ConversationResearchSource>,
) -> Vec<ConversationResearchSource> {
    normalize_sources_with_company(sources, None)
}

pub(super) fn normalize_company_sources(
    sources: Vec<ConversationResearchSource>,
    company_name: &str,
    symbol: &str,
) -> Vec<ConversationResearchSource> {
    normalize_sources_with_company(sources, Some((company_name, symbol)))
}

fn normalize_sources_with_company(
    sources: Vec<ConversationResearchSource>,
    company: Option<(&str, &str)>,
) -> Vec<ConversationResearchSource> {
    let mut seen = HashSet::new();
    sources
        .into_iter()
        .filter_map(|mut source| {
            if source.title.trim().is_empty() || source.snippet.trim().is_empty() {
                return None;
            }
            let mut url = parse_web_url(&source.url)?;
            url.set_fragment(None);
            let inferred_tier = classify_source_url(&url);
            let tier = if inferred_tier == SourceTier::Secondary
                && source.source_tier == SourceTier::Primary.as_str()
                && company.is_some_and(|(name, symbol)| {
                    looks_like_company_investor_relations(&url, name, symbol)
                }) {
                SourceTier::Primary
            } else {
                inferred_tier
            };
            let canonical = url.to_string();
            if !seen.insert(canonical.trim_end_matches('/').to_ascii_lowercase()) {
                return None;
            }
            source.source_tier = tier.as_str().to_string();
            source.title = source.title.trim().to_string();
            source.url = canonical;
            source.snippet = truncate_chars(source.snippet.trim(), MAX_STORED_SOURCE_SNIPPET_CHARS);
            Some(source)
        })
        .take(9)
        .collect()
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut result = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        result.push_str("...");
    }
    result
}

pub(super) async fn verify_source_urls(
    sources: Vec<ConversationResearchSource>,
) -> Vec<ConversationResearchSource> {
    verify_source_urls_with_policy(sources, false).await
}

async fn verify_source_urls_with_policy(
    sources: Vec<ConversationResearchSource>,
    allow_private_networks: bool,
) -> Vec<ConversationResearchSource> {
    join_all(
        sources
            .into_iter()
            .map(|source| verify_source_url(source, allow_private_networks)),
    )
    .await
    .into_iter()
    .flatten()
    .collect()
}

async fn verify_source_url(
    mut source: ConversationResearchSource,
    allow_private_networks: bool,
) -> Option<ConversationResearchSource> {
    let url = parse_web_url_with_policy(&source.url, allow_private_networks)?;
    let host = url.host_str()?.to_ascii_lowercase();
    let port = url.port_or_known_default()?;
    let user_agent = if host_matches_domain(&host, "sec.gov") {
        "Prudentia prudentia-local@example.com"
    } else {
        "Prudentia/0.1 local investment memo"
    };
    let mut builder = Client::builder()
        .redirect(Policy::none())
        .user_agent(user_agent);
    if !allow_private_networks {
        let addresses = tokio::net::lookup_host((host.as_str(), port))
            .await
            .ok()?
            .collect::<Vec<_>>();
        if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
            return None;
        }
        builder = builder.resolve(&host, addresses[0]);
    }
    let client = builder.build().ok()?;
    let direct_response = client
        .get(url)
        .header(RANGE, "bytes=0-262143")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .ok();
    let body = match direct_response {
        Some(response)
            if response.status().is_success()
                && response.url().host_str() == Some(host.as_str()) =>
        {
            read_verified_response(response).await
        }
        Some(response) if response.status().is_redirection() => None,
        Some(response)
            if !allow_private_networks && allows_public_reader_fallback(response.status()) =>
        {
            read_via_public_reader(&source.url).await
        }
        None if !allow_private_networks => read_via_public_reader(&source.url).await,
        _ => None,
    }?;
    let body_text = String::from_utf8_lossy(&body);
    if looks_like_soft_not_found(&body_text) {
        return None;
    }
    if source.source_tier == SourceTier::Community.as_str() {
        if !has_visible_engagement(&body_text) {
            return None;
        }
        source
            .snippet
            .push_str(" Engagement signal verified on the source page.");
    }
    enrich_snippet_from_page(&mut source, &body_text);
    Some(source)
}

fn allows_public_reader_fallback(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::UNAUTHORIZED
            | reqwest::StatusCode::FORBIDDEN
            | reqwest::StatusCode::TOO_MANY_REQUESTS
    )
}

async fn read_verified_response(response: reqwest::Response) -> Option<Vec<u8>> {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !is_supported_research_content(&content_type) {
        return None;
    }
    read_response_prefix(response, 256 * 1024).await
}

async fn read_via_public_reader(source_url: &str) -> Option<Vec<u8>> {
    let source = Url::parse(source_url).ok()?;
    let reader_url = format!("https://r.jina.ai/{}", source.as_str());
    let addresses = tokio::net::lookup_host(("r.jina.ai", 443))
        .await
        .ok()?
        .collect::<Vec<_>>();
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return None;
    }
    let client = Client::builder()
        .redirect(Policy::none())
        .user_agent("Prudentia/0.1 local investment memo")
        .resolve("r.jina.ai", addresses[0])
        .build()
        .ok()?;
    let response = client
        .get(reader_url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = read_verified_response(response).await?;
    let body_text = String::from_utf8_lossy(&body);
    reader_body_matches_source(&body_text, source.as_str()).then_some(body)
}

async fn read_response_prefix(response: reqwest::Response, limit: usize) -> Option<Vec<u8>> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.ok()?;
        let remaining = limit.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if bytes.len() >= limit {
            break;
        }
    }
    Some(bytes)
}

fn is_supported_research_content(content_type: &str) -> bool {
    content_type.starts_with("text/")
        || content_type.contains("application/json")
        || content_type.contains("application/pdf")
        || content_type.contains("application/xml")
}

fn looks_like_soft_not_found(body: &str) -> bool {
    let normalized = body.to_ascii_lowercase();
    [
        "<title>404",
        "page not found",
        "the page you requested does not exist",
        "warning: target url returned error",
        "you've been blocked by network security",
        "页面不存在",
        "该内容已删除",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn is_public_ip(address: std::net::IpAddr) -> bool {
    match address {
        std::net::IpAddr::V4(address) => {
            !(address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || address.is_broadcast()
                || address.is_documentation()
                || address.is_unspecified()
                || address.is_multicast())
        }
        std::net::IpAddr::V6(address) => {
            if let Some(mapped) = address.to_ipv4_mapped() {
                return is_public_ip(std::net::IpAddr::V4(mapped));
            }
            !(address.is_loopback()
                || address.is_unspecified()
                || address.is_unique_local()
                || address.is_unicast_link_local()
                || address.is_multicast())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceTier {
    Primary,
    Secondary,
    Community,
}

impl SourceTier {
    fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Community => "community",
        }
    }
}

#[cfg(test)]
fn source_tier(url: &str) -> &'static str {
    parse_web_url(url)
        .map(|url| classify_source_url(&url))
        .unwrap_or(SourceTier::Secondary)
        .as_str()
}

fn parse_web_url(value: &str) -> Option<Url> {
    parse_web_url_with_policy(value, false)
}

fn parse_web_url_with_policy(value: &str, allow_private_networks: bool) -> Option<Url> {
    let url = Url::parse(value).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let unexpected_port = !matches!(
        (url.scheme(), url.port()),
        ("http", None | Some(80)) | ("https", None | Some(443))
    );
    let private_host = host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.parse::<std::net::IpAddr>().is_ok();
    if !matches!(url.scheme(), "http" | "https")
        || (!allow_private_networks && (unexpected_port || private_host))
    {
        return None;
    }
    (url.username().is_empty() && url.password().is_none()).then_some(url)
}

fn classify_source_url(url: &Url) -> SourceTier {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if COMMUNITY_DOMAINS
        .iter()
        .any(|domain| host_matches_domain(&host, domain))
    {
        SourceTier::Community
    } else if PRIMARY_DOMAINS
        .iter()
        .any(|domain| host_matches_domain(&host, domain))
    {
        SourceTier::Primary
    } else {
        SourceTier::Secondary
    }
}

fn looks_like_company_investor_relations(url: &Url, company_name: &str, symbol: &str) -> bool {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let first_label = host.split('.').next().unwrap_or_default();
    let path = url.path().to_ascii_lowercase();
    let has_ir_signal = matches!(first_label, "ir" | "investor" | "investors")
        || path.contains("investor-relations")
        || path.contains("/investors/");
    if !has_ir_signal {
        return false;
    }
    [company_name, symbol]
        .into_iter()
        .flat_map(|value| value.split(|character: char| !character.is_ascii_alphanumeric()))
        .map(str::to_ascii_lowercase)
        .filter(|token| token.len() >= 3)
        .filter(|token| !matches!(token.as_str(), "inc" | "ltd" | "group" | "holdings"))
        .any(|token| host.contains(&token))
}

fn host_matches_domain(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

fn has_visible_engagement(snippet: &str) -> bool {
    let normalized = snippet.to_ascii_lowercase();
    [
        "vote",
        "comment",
        "like",
        "reply",
        "follower",
        "message volume",
        "buzz",
        "赞",
        "评论",
        "回复",
        "转发",
        "收藏",
        "关注",
        "热度",
    ]
    .iter()
    .any(|term| {
        normalized.match_indices(term).any(|(index, _)| {
            let window = text_window(&normalized, index, term.len(), 72);
            has_engagement_magnitude(window)
        })
    })
}

fn has_engagement_magnitude(value: &str) -> bool {
    let has_non_year_number = value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|token| !token.is_empty())
        .any(|token| {
            token.parse::<u64>().is_ok_and(|number| {
                number > 0 && (token.len() != 4 || !(1900..=2100).contains(&(number as u16)))
            })
        });
    has_non_year_number
        || [
            "numerous",
            "hundreds",
            "thousands",
            "trending",
            "high message volume",
            "top-ranked",
            "热门",
            "高热度",
        ]
        .iter()
        .any(|term| value.contains(term))
}

fn text_window(value: &str, index: usize, term_length: usize, radius: usize) -> &str {
    let mut start = index.saturating_sub(radius);
    while !value.is_char_boundary(start) {
        start += 1;
    }
    let mut end = (index + term_length + radius).min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[start..end]
}

fn reader_body_matches_source(body: &str, expected_source: &str) -> bool {
    let canonical = |value: &str| {
        Url::parse(value.trim()).ok().map(|mut url| {
            url.set_fragment(None);
            url.to_string().trim_end_matches('/').to_string()
        })
    };
    let Some(expected) = canonical(expected_source) else {
        return false;
    };
    body.lines()
        .find_map(|line| line.trim().strip_prefix("URL Source:"))
        .and_then(canonical)
        .is_some_and(|actual| actual == expected)
}

fn enrich_snippet_from_page(source: &mut ConversationResearchSource, body: &str) {
    let Some(summary) = page_summary(body) else {
        return;
    };
    if source.snippet.contains(&summary) {
        return;
    }
    source.snippet.push_str(" Page summary: ");
    source.snippet.push_str(&summary);
}

fn page_summary(body: &str) -> Option<String> {
    let html = Html::parse_document(body);
    let selector = Selector::parse(r#"meta[name="description"], meta[property="og:description"]"#)
        .expect("valid description selector");
    let description = html
        .select(&selector)
        .filter_map(|element| element.value().attr("content"))
        .find(|value| !value.trim().is_empty())
        .or_else(|| body.split_once("Markdown Content:").map(|(_, value)| value))?;
    let normalized = description.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut summary = normalized.chars().take(800).collect::<String>();
    if normalized.chars().count() > 800 {
        summary.push_str("...");
    }
    (!summary.is_empty()).then_some(summary)
}

const COMMUNITY_DOMAINS: [&str; 6] = [
    "xueqiu.com",
    "reddit.com",
    "stocktwits.com",
    "moomoo.com",
    "guba.eastmoney.com",
    "tradingview.com",
];

const PRIMARY_DOMAINS: [&str; 4] = ["sec.gov", "hkexnews.hk", "sse.com.cn", "szse.cn"];

#[cfg(test)]
mod tests {
    use super::{
        allows_public_reader_fallback, has_visible_engagement, is_public_ip,
        normalize_company_sources, normalize_sources, page_summary, reader_body_matches_source,
        source_tier, verify_source_urls_with_policy,
    };
    use axum::{http::StatusCode, response::Redirect, routing::get, Router};
    use tokio::net::TcpListener;

    use crate::ai::ConversationResearchSource;

    #[test]
    fn communities_and_primary_sources_use_exact_hosts() {
        assert_eq!(source_tier("https://xueqiu.com/123/456"), "community");
        assert_eq!(
            source_tier("https://www.reddit.com/r/stocks/abc"),
            "community"
        );
        assert_eq!(
            source_tier("https://reddit.com.evil.example/r/stocks/abc"),
            "secondary"
        );
        assert_eq!(
            source_tier("https://www.sec.gov/Archives/edgar/data/1737806/filing.htm"),
            "primary"
        );
        assert_eq!(
            source_tier("https://investor.pddholdings.com/news/results"),
            "secondary"
        );
        assert_eq!(
            source_tier("https://ir.unrelated.example/news/results"),
            "secondary"
        );
    }

    #[test]
    fn sources_are_deduplicated_validated_and_reclassified() {
        let source = ConversationResearchSource {
            source_tier: "primary".to_string(),
            title: "PDD discussion".to_string(),
            url: "https://www.reddit.com/r/stocks/example".to_string(),
            snippet: "Community viewpoint".to_string(),
        };
        let results = normalize_sources(vec![
            source.clone(),
            source,
            ConversationResearchSource {
                source_tier: "secondary".to_string(),
                title: "Invalid".to_string(),
                url: "javascript:alert(1)".to_string(),
                snippet: "not a web source".to_string(),
            },
        ]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_tier, "community");

        let bounded = normalize_sources(vec![ConversationResearchSource {
            source_tier: "secondary".to_string(),
            title: "Long source".to_string(),
            url: "https://example.com/long".to_string(),
            snippet: "x".repeat(20_000),
        }]);
        assert!(bounded[0].snippet.chars().count() <= 8_003);

        let local_address = normalize_sources(vec![ConversationResearchSource {
            source_tier: "secondary".to_string(),
            title: "Internal service".to_string(),
            url: "http://127.0.0.1/admin".to_string(),
            snippet: "must never be fetched".to_string(),
        }]);
        assert!(local_address.is_empty());
    }

    #[test]
    fn company_investor_relations_requires_both_ir_shape_and_company_identity() {
        let source = |url: &str| ConversationResearchSource {
            source_tier: "primary".to_string(),
            title: "Results".to_string(),
            url: url.to_string(),
            snippet: "Official results".to_string(),
        };

        let results = normalize_company_sources(
            vec![
                source("https://investor.pddholdings.com/news/results"),
                source("https://ir.unrelated.example/news/results"),
            ],
            "PDD Holdings",
            "PDD",
        );

        assert_eq!(results[0].source_tier, "primary");
        assert_eq!(results[1].source_tier, "secondary");
    }

    #[test]
    fn private_network_addresses_are_rejected() {
        assert!(!is_public_ip("127.0.0.1".parse().expect("loopback")));
        assert!(!is_public_ip("10.0.0.5".parse().expect("private")));
        assert!(!is_public_ip(
            "::ffff:127.0.0.1".parse().expect("mapped loopback")
        ));
        assert!(is_public_ip("8.8.8.8".parse().expect("public")));
    }

    #[test]
    fn engagement_requires_a_nearby_non_year_magnitude() {
        assert!(has_visible_engagement("42 comments and 18 votes"));
        assert!(has_visible_engagement("hundreds of comments"));
        assert!(!has_visible_engagement("0 comments and 0 likes"));
        assert!(!has_visible_engagement(&format!(
            "published in 2026 {} comments are available",
            "x".repeat(160)
        )));
        assert!(!has_visible_engagement(
            "published in 2026 with comments enabled"
        ));
    }

    #[test]
    fn public_reader_must_identify_the_original_url() {
        let expected = "https://example.com/report?q=latest";
        assert!(reader_body_matches_source(
            "Title: Report\nURL Source: https://example.com/report?q=latest\nBody",
            expected
        ));
        assert!(!reader_body_matches_source(
            "Title: Report\nURL Source: https://example.com/other\nBody",
            expected
        ));
    }

    #[test]
    fn public_reader_fallback_never_accepts_redirects_or_missing_pages() {
        assert!(allows_public_reader_fallback(StatusCode::FORBIDDEN));
        assert!(!allows_public_reader_fallback(
            StatusCode::TEMPORARY_REDIRECT
        ));
        assert!(!allows_public_reader_fallback(StatusCode::NOT_FOUND));
    }

    #[test]
    fn extracts_bounded_page_descriptions_for_model_context() {
        let summary = page_summary(
            r#"<html><head><meta name="description" content="Revenue grew while margins contracted."></head></html>"#,
        )
        .expect("page summary");
        assert_eq!(summary, "Revenue grew while margins contracted.");
    }

    #[tokio::test]
    async fn only_successful_non_redirecting_sources_with_real_engagement_survive() {
        let app = Router::new()
            .route("/ok", get(|| async { "ok" }))
            .route("/community", get(|| async { "42 comments and 18 votes" }))
            .route("/quiet", get(|| async { "an opinion with no reactions" }))
            .route("/redirect", get(|| async { Redirect::temporary("/ok") }))
            .route("/missing", get(|| async { StatusCode::NOT_FOUND }));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("local address");
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let source = |path: &str, tier: &str| ConversationResearchSource {
            source_tier: tier.to_string(),
            title: path.to_string(),
            url: format!("http://{address}{path}"),
            snippet: "verified source".to_string(),
        };

        let results = verify_source_urls_with_policy(
            vec![
                source("/ok", "secondary"),
                source("/community", "community"),
                source("/quiet", "community"),
                source("/redirect", "secondary"),
                source("/missing", "secondary"),
            ],
            true,
        )
        .await;
        server.abort();

        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|source| source.url.ends_with("/ok")));
        assert!(results
            .iter()
            .any(|source| source.url.ends_with("/community")));
    }
}
