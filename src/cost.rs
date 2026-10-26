/// Fallback cost estimate. The xAI Responses API usually returns an exact
/// `usage.cost_in_usd_ticks` value, which is always preferred.
const PRICES: &[(&str, f64, f64, f64)] = &[
    ("grok-4.7", 2.0, 0.5, 6.0),
    ("grok-4.6", 2.0, 0.5, 6.0),
    ("grok-4.5", 2.0, 0.3, 6.0),
    ("grok-4.3", 1.25, 0.2, 2.5),
    ("grok-4.20-0309-reasoning", 1.25, 0.2, 2.5),
    ("grok-4.20-0309-non-reasoning", 1.25, 0.2, 2.5),
    ("grok-4.20-multi-agent-0309", 1.25, 0.2, 2.5),
    ("grok-build-0.1", 1.0, 0.2, 2.0),
];

const X_POST_USD: f64 = 0.005;
const X_USER_USD: f64 = 0.01;

pub struct CostInput<'a> {
    pub model: &'a str,
    pub input: i64,
    pub cached: i64,
    pub output: i64,
    pub x_posts: i64,
    pub x_users: i64,
}

pub fn estimate_cost(input: &CostInput<'_>) -> Option<f64> {
    let (_, input_price, cached_price, output_price) =
        PRICES.iter().find(|(model, ..)| *model == input.model)?;

    let multiplier = if input.input >= 200_000 { 2.0 } else { 1.0 };
    let uncached = (input.input - input.cached).max(0) as f64;
    let token_cost = (uncached * input_price
        + input.cached as f64 * cached_price
        + input.output as f64 * output_price)
        / 1_000_000.0
        * multiplier;
    let tool_cost = input.x_posts as f64 * X_POST_USD + input.x_users as f64 * X_USER_USD;

    Some(round_usd(token_cost + tool_cost))
}

pub fn round_usd(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

pub fn format_usd(value: Option<f64>) -> String {
    match value {
        None => "unknown".to_string(),
        Some(value) => format!("${value:.4}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_tokens_and_x_search_items() {
        let cost = estimate_cost(&CostInput {
            model: "grok-4.7",
            input: 1000,
            cached: 100,
            output: 200,
            x_posts: 3,
            x_users: 0,
        });
        assert_eq!(cost, Some(0.01805));
    }

    #[test]
    fn doubles_token_rates_for_long_context() {
        let cost = estimate_cost(&CostInput {
            model: "grok-4.7",
            input: 200_000,
            cached: 0,
            output: 0,
            x_posts: 0,
            x_users: 0,
        });
        assert_eq!(cost, Some(0.8));
    }

    #[test]
    fn returns_none_for_unknown_models() {
        let cost = estimate_cost(&CostInput {
            model: "mystery-model",
            input: 1,
            cached: 0,
            output: 1,
            x_posts: 0,
            x_users: 0,
        });
        assert_eq!(cost, None);
    }

    #[test]
    fn rounds_to_six_decimals() {
        assert_eq!(round_usd(0.123456789), 0.123457);
    }

    #[test]
    fn formats_usd() {
        assert_eq!(format_usd(Some(0.059564)), "$0.0596");
        assert_eq!(format_usd(None), "unknown");
    }
}
