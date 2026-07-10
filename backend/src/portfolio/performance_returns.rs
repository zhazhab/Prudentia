fn cash_flow_sum_between(cash_flows: &[PerformanceCashFlowRow], start_at: &str, end_at: &str) -> f64 {
    cash_flows
        .iter()
        .filter(|flow| flow.occurred_at.as_str() > start_at && flow.occurred_at.as_str() <= end_at)
        .map(|flow| flow.amount_base)
        .sum()
}

fn period_return_factor(start_value: f64, end_value: f64, net_cash_flow: f64) -> Option<f64> {
    if start_value.abs() < f64::EPSILON {
        return None;
    }
    let factor = (end_value - net_cash_flow) / start_value;
    if factor <= 0.0 {
        return None;
    }
    Some(factor)
}

fn annualized_return_from_period_return(
    return_pct: f64,
    start_at: &str,
    end_at: &str,
) -> Option<f64> {
    if return_pct.abs() < f64::EPSILON {
        return Some(0.0);
    }

    let start_at = chrono::DateTime::parse_from_rfc3339(start_at).ok()?;
    let end_at = chrono::DateTime::parse_from_rfc3339(end_at).ok()?;
    let elapsed_seconds = end_at.signed_duration_since(start_at).num_seconds();
    if elapsed_seconds <= 0 {
        return None;
    }

    let elapsed_days = elapsed_seconds as f64 / 86_400.0;
    let ratio = 1.0 + return_pct;
    if ratio <= 0.0 {
        return None;
    }

    Some(ratio.powf(365.25 / elapsed_days) - 1.0)
}
