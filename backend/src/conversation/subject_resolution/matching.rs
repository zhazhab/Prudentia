pub(super) fn extract_company_hint(message: &str) -> Option<String> {
    const ACTION_MARKERS: &[&str] = &[
        "分析一下",
        "研究一下",
        "了解一下",
        "帮我分析",
        "帮我研究",
        "分析下",
        "研究下",
        "分析",
        "研究",
        "了解",
        "看看",
        "看一下",
        "聊聊",
        "说说",
        "what about",
        "analyze",
        "analyse",
        "research",
    ];
    const TOPIC_MARKERS: &[&str] = &[
        "的商业模式",
        "的护城河",
        "的财报",
        "怎么样",
        "近五年",
        "近十年",
        "最新",
        "商业模式",
        "护城河",
        "财报",
        "年报",
        "季报",
        "竞争",
        "盈利",
        "利润",
        "风险",
        "为什么",
        "是否",
        "如何",
        "？",
        "?",
        "，",
        ",",
        "。",
        "\n",
    ];
    const SUBJECT_TOPIC_MARKERS: &[&str] = &[
        "的商业模式",
        "的护城河",
        "的财报",
        "怎么样",
        "近五年",
        "近十年",
        "最新财报",
        "商业模式",
        "护城河",
        "财报",
        "年报",
        "季报",
    ];
    let trimmed = message.trim();
    if looks_like_security_code(trimmed) {
        return Some(trimmed.to_string());
    }
    let normalized = trimmed.to_ascii_lowercase();
    let mut candidate = ACTION_MARKERS
        .iter()
        .filter_map(|marker| normalized.find(marker).map(|index| (index, *marker)))
        .min_by(|(left_index, left_marker), (right_index, right_marker)| {
            left_index
                .cmp(right_index)
                .then_with(|| right_marker.len().cmp(&left_marker.len()))
        })
        .map(|(index, marker)| &normalized[index + marker.len()..])
        .or_else(|| {
            SUBJECT_TOPIC_MARKERS
                .iter()
                .filter_map(|marker| normalized.find(marker).filter(|index| *index > 0))
                .min()
                .map(|index| &normalized[..index])
        })?
        .trim();
    for prefix in ["一下", "下", "请问", "关于", "对", "这家", "那家"] {
        if let Some(stripped) = candidate.strip_prefix(prefix) {
            candidate = stripped.trim();
        }
    }
    let end = TOPIC_MARKERS
        .iter()
        .filter_map(|marker| candidate.find(marker))
        .min()
        .unwrap_or(candidate.len());
    let mut hint = candidate[..end]
        .trim_matches(|character: char| {
            character.is_whitespace()
                || matches!(character, ':' | '：' | '-' | '－' | '(' | ')' | '（' | '）')
        })
        .to_string();
    if let Some(stripped) = hint.strip_suffix("公司") {
        hint = stripped.trim().to_string();
    }
    valid_company_hint(&hint).then_some(hint)
}

pub(super) fn looks_like_security_code(value: &str) -> bool {
    let length = value.chars().count();
    (2..=16).contains(&length)
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
        && (value.chars().any(|character| character.is_ascii_digit())
            || value.contains('.')
            || (value
                .chars()
                .any(|character| character.is_ascii_alphabetic())
                && value
                    .chars()
                    .filter(|character| character.is_ascii_alphabetic())
                    .all(|character| character.is_ascii_uppercase())))
}

pub(super) fn valid_company_hint(value: &str) -> bool {
    let normalized = normalize_text(value);
    let length = normalized.chars().count();
    let generic = [
        "它",
        "这个",
        "这家",
        "那家",
        "公司",
        "企业",
        "我的持仓",
        "持仓",
        "组合",
        "收益",
        "投资体系",
        "规则",
        "行业",
        "市场",
        "portfolio",
        "market",
        "industry",
    ];
    let instruction_fragments = [
        "上一轮",
        "公司看法",
        "明确结论",
        "新结论",
        "更新",
        "沉淀",
        "补充",
        "提议",
        "复盘",
        "重新",
        "继续",
        "不增加",
    ];
    let has_sentence_punctuation = normalized.chars().any(|character| {
        matches!(
            character,
            '，' | ',' | '。' | '！' | '!' | '？' | '?' | ';' | '；' | '\n'
        )
    });
    (2..=32).contains(&length)
        && !generic.contains(&normalized.as_str())
        && !instruction_fragments
            .iter()
            .any(|fragment| normalized.contains(fragment))
        && !has_sentence_punctuation
}

pub(super) fn contains_symbol(message: &str, symbol: &str) -> bool {
    let symbol = symbol.to_ascii_uppercase();
    let base = symbol.split('.').next().unwrap_or(&symbol);
    message
        .to_ascii_uppercase()
        .split(|character: char| {
            !(character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
        })
        .any(|token| token == symbol || (base.len() >= 3 && token == base))
}

pub(super) fn is_strong_symbol_reference(
    message: &str,
    symbol: &str,
    has_company_hint: bool,
) -> bool {
    if !contains_symbol(message, symbol) {
        return false;
    }
    if has_company_hint
        || symbol.contains('.')
        || symbol.chars().any(|character| character.is_ascii_digit())
        || has_contextual_symbol_reference(message, symbol)
    {
        return true;
    }
    message
        .split(|character: char| {
            !(character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
        })
        .any(|token| token == symbol)
}

fn has_contextual_symbol_reference(message: &str, symbol: &str) -> bool {
    const BEFORE_MARKERS: &[&str] = &[
        "投资",
        "买入",
        "卖出",
        "持有",
        "关注",
        "分析",
        "分析一下",
        "研究",
        "研究一下",
        "介绍",
        "介绍一下",
        "了解",
        "了解一下",
        "关于",
        "invest in",
        "buy",
        "sell",
        "hold",
        "analyze",
        "analyse",
        "research",
        "review",
        "about",
    ];
    const AFTER_MARKERS: &[&str] = &[
        "公司",
        "企业",
        "股票",
        "的",
        "怎么样",
        "是什么",
        "护城河",
        "商业模式",
        "财报",
        "年报",
        "风险",
        "company",
        "stock",
        "moat",
        "business model",
        "earnings",
        "filing",
        "risk",
    ];
    let normalized = message.to_ascii_lowercase();
    let symbol = symbol.to_ascii_lowercase();
    let base = symbol.split('.').next().unwrap_or(&symbol);
    normalized.match_indices(base).any(|(start, matched)| {
        let end = start + matched.len();
        let previous = normalized[..start].chars().next_back();
        let next = normalized[end..].chars().next();
        let is_boundary = |character: Option<char>| {
            character.is_none_or(|character| {
                !(character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
            })
        };
        if !is_boundary(previous) || !is_boundary(next) {
            return false;
        }
        let prefix = normalized[..start].trim_end_matches(|character: char| {
            character.is_whitespace()
                || matches!(character, ':' | '：' | ',' | '，' | '-' | '－' | '(' | '（')
        });
        let suffix = normalized[end..].trim_start_matches(|character: char| {
            character.is_whitespace()
                || matches!(character, ':' | '：' | ',' | '，' | '-' | '－' | ')' | '）')
        });
        BEFORE_MARKERS.iter().any(|marker| prefix.ends_with(marker))
            || AFTER_MARKERS
                .iter()
                .any(|marker| suffix.starts_with(marker))
    })
}

pub(super) fn normalize_text(value: &str) -> String {
    value.trim().to_lowercase()
}

pub(super) fn trim_company_name(value: &str) -> &str {
    value.trim_matches(|character: char| {
        character.is_whitespace()
            || matches!(character, '.' | ',' | '-' | '－' | '(' | ')' | '（' | '）')
    })
}

pub(super) fn valid_company_alias(value: &str) -> bool {
    let length = value.chars().count();
    if value.is_ascii() {
        length >= 3
    } else {
        length >= 2
    }
}

pub(super) fn is_secondary_counter(name: &str) -> bool {
    let normalized = normalize_text(name);
    ["－ｒ", "-r", "－ｗｒ", "-wr"]
        .iter()
        .any(|suffix| normalized.ends_with(suffix))
}

pub(super) fn is_derivative_or_fund(name: &str) -> bool {
    let normalized = normalize_text(name);
    [
        "购", "沽", "牛", "熊", "中银", "瑞银", "摩通", "法兴", "汇丰", "信证", "麦银", "etf",
        "基金",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

pub(super) fn contains_any(value: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| value.contains(candidate))
}
