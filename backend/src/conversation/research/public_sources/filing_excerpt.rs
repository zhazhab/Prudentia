use scraper::Html;

const BUSINESS_ANCHORS: [(&str, i32); 6] = [
    ("our company", 36),
    ("our business", 34),
    ("business overview", 30),
    ("how we generate revenue", 28),
    ("business model", 26),
    ("information on the company", 12),
];
const BUSINESS_EVIDENCE: [&str; 14] = [
    "customer",
    "consumer",
    "merchant",
    "supplier",
    "product",
    "service",
    "platform",
    "revenue",
    "fee",
    "transaction",
    "advertising",
    "fulfillment",
    "marketplace",
    "segment",
];
const COMPETITION_ANCHORS: [(&str, i32); 5] = [
    ("competitive landscape", 42),
    ("our competitors", 38),
    ("intense competition", 36),
    ("competition", 30),
    ("competitive", 14),
];
const COMPETITION_EVIDENCE: [&str; 15] = [
    "competitor",
    "compete",
    "intense",
    "buyer",
    "merchant",
    "supplier",
    "switching",
    "multi-hom",
    "price",
    "subsid",
    "scale",
    "brand",
    "traffic",
    "logistics",
    "barrier",
];
const MONETIZATION_ANCHORS: [(&str, i32); 5] = [
    ("how we generate revenue", 42),
    ("monetization", 38),
    ("revenue model", 36),
    ("online marketing services", 30),
    ("transaction services", 28),
];
const MONETIZATION_EVIDENCE: [&str; 12] = [
    "revenue",
    "fee",
    "commission",
    "advertising",
    "transaction",
    "take rate",
    "pricing",
    "merchant",
    "customer",
    "service",
    "subscription",
    "payment",
];
const PROFIT_ANCHORS: [(&str, i32); 6] = [
    ("sales and marketing expenses", 42),
    ("cost of revenues", 40),
    ("gross profit", 34),
    ("operating profit", 34),
    ("operating income", 32),
    ("operating expenses", 26),
];
const PROFIT_EVIDENCE: [&str; 15] = [
    "margin",
    "cost",
    "expense",
    "marketing",
    "acquisition",
    "subsid",
    "fulfillment",
    "logistics",
    "research and development",
    "working capital",
    "capital expenditure",
    "profit",
    "income",
    "leverage",
    "reinvest",
];
const OWNER_ECONOMICS_ANCHORS: [(&str, i32); 6] = [
    ("capital expenditures", 40),
    ("share-based compensation", 38),
    ("free cash flow", 36),
    ("cash flows from operating activities", 34),
    ("return on invested capital", 32),
    ("retained earnings", 28),
];
const OWNER_ECONOMICS_EVIDENCE: [&str; 13] = [
    "maintenance",
    "growth",
    "capital expenditure",
    "cash flow",
    "share-based",
    "dilut",
    "weighted average shares",
    "return on invested capital",
    "retained earnings",
    "reinvest",
    "acquisition",
    "working capital",
    "capitalized",
];
const STEWARDSHIP_ANCHORS: [(&str, i32); 6] = [
    ("capital allocation", 42),
    ("executive compensation", 40),
    ("corporate governance", 36),
    ("share repurchase", 34),
    ("acquisitions", 30),
    ("management", 16),
];
const STEWARDSHIP_EVIDENCE: [&str; 15] = [
    "incentive",
    "compensation",
    "performance measure",
    "related party",
    "succession",
    "founder",
    "acquisition",
    "repurchase",
    "dividend",
    "debt",
    "reinvest",
    "retained earnings",
    "governance",
    "internal control",
    "ownership",
];
const RESILIENCE_ANCHORS: [(&str, i32); 5] = [
    ("liquidity and capital resources", 42),
    ("debt maturities", 40),
    ("contractual obligations", 36),
    ("off-balance sheet arrangements", 34),
    ("working capital", 20),
];
const RESILIENCE_EVIDENCE: [&str; 13] = [
    "cash and cash equivalents",
    "liquidity",
    "matur",
    "refinanc",
    "credit facility",
    "working capital",
    "contingent",
    "off-balance",
    "guarantee",
    "merchant funds",
    "customer funds",
    "regulatory",
    "obligation",
];

struct ExcerptSpec<'a> {
    label: &'a str,
    anchors: &'a [(&'a str, i32)],
    evidence_terms: &'a [&'a str],
    limit: usize,
}

pub(super) fn official_document_excerpt(body: &str) -> String {
    let document = Html::parse_document(body);
    let text = document
        .root_element()
        .text()
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ");
    let specs = [
        ExcerptSpec {
            label: "",
            anchors: &BUSINESS_ANCHORS,
            evidence_terms: &BUSINESS_EVIDENCE,
            limit: 850,
        },
        ExcerptSpec {
            label: "Competition evidence: ",
            anchors: &COMPETITION_ANCHORS,
            evidence_terms: &COMPETITION_EVIDENCE,
            limit: 650,
        },
        ExcerptSpec {
            label: "Monetization evidence: ",
            anchors: &MONETIZATION_ANCHORS,
            evidence_terms: &MONETIZATION_EVIDENCE,
            limit: 450,
        },
        ExcerptSpec {
            label: "Profit-engine evidence: ",
            anchors: &PROFIT_ANCHORS,
            evidence_terms: &PROFIT_EVIDENCE,
            limit: 600,
        },
        ExcerptSpec {
            label: "Owner-economics evidence: ",
            anchors: &OWNER_ECONOMICS_ANCHORS,
            evidence_terms: &OWNER_ECONOMICS_EVIDENCE,
            limit: 650,
        },
        ExcerptSpec {
            label: "Management, incentives, and capital-allocation evidence: ",
            anchors: &STEWARDSHIP_ANCHORS,
            evidence_terms: &STEWARDSHIP_EVIDENCE,
            limit: 650,
        },
        ExcerptSpec {
            label: "Financial-resilience evidence: ",
            anchors: &RESILIENCE_ANCHORS,
            evidence_terms: &RESILIENCE_EVIDENCE,
            limit: 600,
        },
    ];
    let mut selected_indexes = Vec::new();
    let mut excerpt = String::new();
    for spec in specs {
        let Some((index, candidate)) = best_window(&text, &spec) else {
            continue;
        };
        if selected_indexes
            .iter()
            .any(|selected| index.abs_diff(*selected) < 700)
        {
            continue;
        }
        if !excerpt.is_empty() {
            excerpt.push(' ');
        }
        excerpt.push_str(spec.label);
        excerpt.push_str(&candidate);
        selected_indexes.push(index);
    }
    if excerpt.is_empty() {
        excerpt = text;
    }
    truncate_chars(&excerpt, 4_500)
}

fn best_window(text: &str, spec: &ExcerptSpec<'_>) -> Option<(usize, String)> {
    let normalized = text.to_ascii_lowercase();
    let mut best = None::<(i32, usize, String)>;
    for (anchor, anchor_score) in spec.anchors {
        for (index, _) in normalized.match_indices(anchor) {
            let candidate = text[index..].chars().take(spec.limit).collect::<String>();
            let normalized_candidate = candidate.to_ascii_lowercase();
            let evidence_score = spec
                .evidence_terms
                .iter()
                .map(|term| normalized_candidate.matches(term).count().min(4) as i32)
                .sum::<i32>();
            let contents_penalty = if normalized_candidate
                .chars()
                .take(400)
                .collect::<String>()
                .contains("table of contents")
            {
                30
            } else {
                0
            };
            let score = anchor_score + evidence_score - contents_penalty;
            if best.as_ref().is_none_or(|(best_score, best_index, _)| {
                score > *best_score || (score == *best_score && index > *best_index)
            }) {
                best = Some((score, index, candidate));
            }
        }
    }
    best.map(|(_, index, candidate)| (index, candidate))
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut result = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        result.push_str("...");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::official_document_excerpt;

    #[test]
    fn extracts_owner_stewardship_and_resilience_evidence_within_the_existing_limit() {
        let spacer = " detail".repeat(140);
        let body = format!(
            r#"<html><body>
            <p>Our Business serves customers and merchants through products, services, fees, transactions, and fulfillment.{spacer}</p>
            <p>Competitive Landscape includes competitors, switching behavior, subsidies, scale, traffic, and barriers.{spacer}</p>
            <p>Capital expenditures include maintenance and growth investment. Cash flows from operating activities, share-based compensation, diluted weighted average shares, retained earnings, and reinvestment determine owner economics.{spacer}</p>
            <p>Capital allocation and executive compensation link incentive metrics to management decisions. Governance, succession, acquisitions, debt, dividends, and share repurchases affect per-share outcomes.{spacer}</p>
            <p>Liquidity and capital resources include cash and cash equivalents, debt maturities, refinancing, contractual obligations, working capital, and contingent off-balance-sheet liabilities.{spacer}</p>
            </body></html>"#
        );

        let excerpt = official_document_excerpt(&body);

        assert!(excerpt.contains("Owner-economics evidence:"));
        assert!(excerpt.contains("share-based compensation"));
        assert!(excerpt.contains("Management, incentives, and capital-allocation evidence:"));
        assert!(excerpt.contains("succession"));
        assert!(excerpt.contains("Financial-resilience evidence:"));
        assert!(excerpt.contains("debt maturities"));
        assert!(excerpt.chars().count() <= 4_503);
    }
}
