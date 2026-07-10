use uuid::Uuid;

use super::CliProviderKind;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CliTokenUsage {
    input_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    reasoning_output_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

pub(super) fn log_cli_token_usage(
    invocation_id: Uuid,
    provider: CliProviderKind,
    program: &str,
    output: &str,
) {
    let json_event_count = output
        .lines()
        .filter(|line| serde_json::from_str::<serde_json::Value>(line.trim()).is_ok())
        .count();

    if let Some(usage) = parse_cli_token_usage(output) {
        tracing::info!(
            invocation_id = %invocation_id,
            provider = provider.as_str(),
            program,
            token_usage_available = true,
            input_tokens = usage.input_tokens.unwrap_or_default(),
            cached_input_tokens = usage.cached_input_tokens.unwrap_or_default(),
            output_tokens = usage.output_tokens.unwrap_or_default(),
            reasoning_output_tokens = usage.reasoning_output_tokens.unwrap_or_default(),
            total_tokens = usage.total_tokens.unwrap_or_default(),
            json_event_count,
            "AI CLI token usage"
        );
    } else {
        tracing::info!(
            invocation_id = %invocation_id,
            provider = provider.as_str(),
            program,
            token_usage_available = false,
            json_event_count,
            "AI CLI token usage unavailable"
        );
    }
}

fn parse_cli_token_usage(output: &str) -> Option<CliTokenUsage> {
    let mut usage = None;

    for line in output.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };

        if value.get("type").and_then(|field| field.as_str()) == Some("turn.completed") {
            if let Some(parsed) = value.get("usage").and_then(CliTokenUsage::from_value) {
                usage = Some(parsed);
            }
            continue;
        }

        if value.get("type").and_then(|field| field.as_str()) == Some("event_msg") {
            let Some(payload) = value.get("payload") else {
                continue;
            };
            if payload.get("type").and_then(|field| field.as_str()) != Some("token_count") {
                continue;
            }
            let parsed = payload
                .get("info")
                .and_then(|info| info.get("last_token_usage"))
                .and_then(CliTokenUsage::from_value)
                .or_else(|| {
                    payload
                        .get("info")
                        .and_then(|info| info.get("total_token_usage"))
                        .and_then(CliTokenUsage::from_value)
                });
            if let Some(parsed) = parsed {
                usage = Some(parsed);
            }
        }
    }

    usage
}

impl CliTokenUsage {
    fn from_value(value: &serde_json::Value) -> Option<Self> {
        let input_tokens = number_field(value, &["input_tokens", "prompt_tokens"]);
        let cached_input_tokens =
            number_field(value, &["cached_input_tokens", "cached_prompt_tokens"]);
        let output_tokens = number_field(value, &["output_tokens", "completion_tokens"]);
        let reasoning_output_tokens =
            number_field(value, &["reasoning_output_tokens", "reasoning_tokens"]);
        let total_tokens = number_field(value, &["total_tokens"]).or_else(|| {
            input_tokens
                .zip(output_tokens)
                .map(|(input, output)| input + output)
        });

        let usage = Self {
            input_tokens,
            cached_input_tokens,
            output_tokens,
            reasoning_output_tokens,
            total_tokens,
        };

        (usage.input_tokens.is_some()
            || usage.cached_input_tokens.is_some()
            || usage.output_tokens.is_some()
            || usage.reasoning_output_tokens.is_some()
            || usage.total_tokens.is_some())
        .then_some(usage)
    }
}

fn number_field(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|field| field.as_u64()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_json_usage_from_turn_completed_event() {
        let output = r#"{"type":"thread.started","thread_id":"thread-1"}
{"type":"turn.completed","usage":{"input_tokens":14785,"cached_input_tokens":5504,"output_tokens":427,"reasoning_output_tokens":416}}"#;

        let usage = parse_cli_token_usage(output).expect("usage should be parsed");

        assert_eq!(usage.input_tokens, Some(14785));
        assert_eq!(usage.cached_input_tokens, Some(5504));
        assert_eq!(usage.output_tokens, Some(427));
        assert_eq!(usage.reasoning_output_tokens, Some(416));
        assert_eq!(usage.total_tokens, Some(15212));
    }

    #[test]
    fn missing_cli_usage_returns_none() {
        let output = r#"{"type":"thread.started","thread_id":"thread-1"}"#;

        assert_eq!(parse_cli_token_usage(output), None);
    }
}
