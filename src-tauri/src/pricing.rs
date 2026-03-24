#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TokenBreakdown {
    pub input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub other_tokens: u64,
}

impl TokenBreakdown {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.cache_creation_input_tokens)
            .saturating_add(self.cached_input_tokens)
            .saturating_add(self.output_tokens)
            .saturating_add(self.other_tokens)
    }
}

#[derive(Clone, Copy, Debug)]
struct ModelPricing {
    input_per_million: f64,
    cached_input_per_million: Option<f64>,
    cache_write_input_per_million: Option<f64>,
    output_per_million: f64,
}

const MODEL_PRICING: &[(&str, ModelPricing)] = &[
    (
        "gpt-5.4",
        ModelPricing {
            input_per_million: 2.50,
            cached_input_per_million: Some(0.25),
            cache_write_input_per_million: None,
            output_per_million: 15.00,
        },
    ),
    (
        "gpt-5.4-mini",
        ModelPricing {
            input_per_million: 0.75,
            cached_input_per_million: Some(0.075),
            cache_write_input_per_million: None,
            output_per_million: 4.50,
        },
    ),
    (
        "gpt-5.4-nano",
        ModelPricing {
            input_per_million: 0.20,
            cached_input_per_million: Some(0.02),
            cache_write_input_per_million: None,
            output_per_million: 1.25,
        },
    ),
    (
        "gpt-5.1",
        ModelPricing {
            input_per_million: 1.25,
            cached_input_per_million: Some(0.125),
            cache_write_input_per_million: None,
            output_per_million: 10.00,
        },
    ),
    (
        "gpt-5.1-mini",
        ModelPricing {
            input_per_million: 0.25,
            cached_input_per_million: Some(0.025),
            cache_write_input_per_million: None,
            output_per_million: 2.00,
        },
    ),
    (
        "gpt-5.1-codex",
        ModelPricing {
            input_per_million: 1.25,
            cached_input_per_million: Some(0.125),
            cache_write_input_per_million: None,
            output_per_million: 10.00,
        },
    ),
    (
        "gpt-5.1-codex-max",
        ModelPricing {
            input_per_million: 1.25,
            cached_input_per_million: Some(0.125),
            cache_write_input_per_million: None,
            output_per_million: 10.00,
        },
    ),
    (
        "gpt-5.3-codex",
        ModelPricing {
            input_per_million: 1.75,
            cached_input_per_million: Some(0.175),
            cache_write_input_per_million: None,
            output_per_million: 14.00,
        },
    ),
    (
        "claude-haiku-4.5",
        ModelPricing {
            input_per_million: 1.00,
            cached_input_per_million: Some(0.10),
            cache_write_input_per_million: Some(1.25),
            output_per_million: 5.00,
        },
    ),
    (
        "claude-sonnet-4",
        ModelPricing {
            input_per_million: 3.00,
            cached_input_per_million: Some(0.30),
            cache_write_input_per_million: Some(3.75),
            output_per_million: 15.00,
        },
    ),
    (
        "claude-sonnet-4.5",
        ModelPricing {
            input_per_million: 3.00,
            cached_input_per_million: Some(0.30),
            cache_write_input_per_million: Some(3.75),
            output_per_million: 15.00,
        },
    ),
    (
        "claude-sonnet-4.6",
        ModelPricing {
            input_per_million: 3.00,
            cached_input_per_million: Some(0.30),
            cache_write_input_per_million: Some(3.75),
            output_per_million: 15.00,
        },
    ),
    (
        "claude-opus-4",
        ModelPricing {
            input_per_million: 15.00,
            cached_input_per_million: Some(1.50),
            cache_write_input_per_million: Some(18.75),
            output_per_million: 75.00,
        },
    ),
    (
        "claude-opus-4.5",
        ModelPricing {
            input_per_million: 5.00,
            cached_input_per_million: Some(0.50),
            cache_write_input_per_million: Some(6.25),
            output_per_million: 25.00,
        },
    ),
    (
        "claude-opus-4.6",
        ModelPricing {
            input_per_million: 5.00,
            cached_input_per_million: Some(0.50),
            cache_write_input_per_million: Some(6.25),
            output_per_million: 25.00,
        },
    ),
    (
        "gemini-2.5-pro",
        ModelPricing {
            input_per_million: 1.25,
            cached_input_per_million: Some(0.125),
            cache_write_input_per_million: None,
            output_per_million: 10.00,
        },
    ),
    (
        "gemini-2.5-flash",
        ModelPricing {
            input_per_million: 0.30,
            cached_input_per_million: Some(0.03),
            cache_write_input_per_million: None,
            output_per_million: 2.50,
        },
    ),
    (
        "gemini-2.5-flash-lite",
        ModelPricing {
            input_per_million: 0.10,
            cached_input_per_million: Some(0.01),
            cache_write_input_per_million: None,
            output_per_million: 0.40,
        },
    ),
];

pub fn normalize_model_name(model: &str) -> Option<String> {
    let mut normalized = model.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "unknown" || normalized == "<synthetic>" {
        return None;
    }

    for prefix in ["openai/", "anthropic/", "google/"] {
        if let Some(rest) = normalized.strip_prefix(prefix) {
            normalized = rest.to_string();
            break;
        }
    }

    if let Some((head, _)) = normalized.split_once('@') {
        normalized = head.to_string();
    }

    normalized = normalized.replace('_', "-");

    if let Some(canonical) = normalize_openai_model(&normalized) {
        return Some(canonical.to_string());
    }

    if let Some(canonical) = normalize_claude_model(&normalized) {
        return Some(canonical.to_string());
    }

    if let Some(canonical) = normalize_gemini_model(&normalized) {
        return Some(canonical.to_string());
    }

    MODEL_PRICING
        .iter()
        .find(|(name, _)| *name == normalized)
        .map(|(name, _)| (*name).to_string())
}

pub fn estimate_cost_usd(model: &str, breakdown: TokenBreakdown) -> Option<f64> {
    if breakdown.other_tokens > 0 {
        return None;
    }

    let canonical = normalize_model_name(model)?;
    let pricing = MODEL_PRICING
        .iter()
        .find(|(name, _)| *name == canonical)
        .map(|(_, pricing)| pricing)?;

    let cache_write_rate = pricing
        .cache_write_input_per_million
        .unwrap_or(pricing.input_per_million);
    let cached_input_rate = pricing
        .cached_input_per_million
        .unwrap_or(pricing.input_per_million);

    let total = (breakdown.input_tokens as f64 * pricing.input_per_million)
        + (breakdown.cache_creation_input_tokens as f64 * cache_write_rate)
        + (breakdown.cached_input_tokens as f64 * cached_input_rate)
        + (breakdown.output_tokens as f64 * pricing.output_per_million);

    Some(total / 1_000_000.0)
}

fn normalize_openai_model(model: &str) -> Option<&'static str> {
    match model {
        "gpt-5.4" | "gpt-5-4" | "gpt-5.4-thinking" | "gpt-5-4-thinking" => Some("gpt-5.4"),
        "gpt-5.4-mini" | "gpt-5-4-mini" => Some("gpt-5.4-mini"),
        "gpt-5.4-nano" | "gpt-5-4-nano" => Some("gpt-5.4-nano"),
        "gpt-5.1" | "gpt-5-1" | "gpt-5.1-thinking" | "gpt-5-1-thinking" => Some("gpt-5.1"),
        "gpt-5.1-mini" | "gpt-5-1-mini" => Some("gpt-5.1-mini"),
        "gpt-5.1-codex" | "gpt-5-1-codex" => Some("gpt-5.1-codex"),
        "gpt-5.1-codex-max" | "gpt-5-1-codex-max" => Some("gpt-5.1-codex-max"),
        "gpt-5.3-codex" | "gpt-5-3-codex" => Some("gpt-5.3-codex"),
        _ => None,
    }
}

fn normalize_claude_model(model: &str) -> Option<&'static str> {
    let model = strip_claude_date_suffix(model);
    match model {
        "claude-haiku-4.5" | "claude-haiku-4-5" => Some("claude-haiku-4.5"),
        "claude-sonnet-4" => Some("claude-sonnet-4"),
        "claude-sonnet-4.5" | "claude-sonnet-4-5" => Some("claude-sonnet-4.5"),
        "claude-sonnet-4.6" | "claude-sonnet-4-6" => Some("claude-sonnet-4.6"),
        "claude-opus-4" => Some("claude-opus-4"),
        "claude-opus-4.5" | "claude-opus-4-5" => Some("claude-opus-4.5"),
        "claude-opus-4.6" | "claude-opus-4-6" => Some("claude-opus-4.6"),
        _ => None,
    }
}

fn strip_claude_date_suffix(model: &str) -> &str {
    let Some((head, tail)) = model.rsplit_once('-') else {
        return model;
    };

    if tail.len() == 8 && tail.chars().all(|ch| ch.is_ascii_digit()) {
        head
    } else {
        model
    }
}

fn normalize_gemini_model(model: &str) -> Option<&'static str> {
    match model {
        "gemini-2.5-pro" => Some("gemini-2.5-pro"),
        "gemini-2.5-flash" => Some("gemini-2.5-flash"),
        "gemini-2.5-flash-lite" | "gemini-2.5-flash-lite-preview-09-2025" => {
            Some("gemini-2.5-flash-lite")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(left: f64, right: f64) {
        let delta = (left - right).abs();
        assert!(delta < 1e-9, "left={left}, right={right}, delta={delta}");
    }

    #[test]
    fn normalizes_openai_and_anthropic_aliases() {
        assert_eq!(normalize_model_name("gpt-5-4-thinking").as_deref(), Some("gpt-5.4"));
        assert_eq!(
            normalize_model_name("anthropic/claude-sonnet-4.6").as_deref(),
            Some("claude-sonnet-4.6")
        );
        assert_eq!(
            normalize_model_name("claude-sonnet-4-6").as_deref(),
            Some("claude-sonnet-4.6")
        );
    }

    #[test]
    fn estimates_gpt_5_4_cost_with_cached_input() {
        let cost = estimate_cost_usd(
            "gpt-5.4",
            TokenBreakdown {
                input_tokens: 1_000,
                cached_input_tokens: 500,
                output_tokens: 250,
                ..TokenBreakdown::default()
            },
        )
        .expect("gpt-5.4 should be priced");

        approx_eq(cost, 0.006_375);
    }

    #[test]
    fn estimates_claude_sonnet_cost_with_cache_write_and_read() {
        let cost = estimate_cost_usd(
            "claude-sonnet-4-6",
            TokenBreakdown {
                input_tokens: 1_000,
                cache_creation_input_tokens: 500,
                cached_input_tokens: 200,
                output_tokens: 100,
                ..TokenBreakdown::default()
            },
        )
        .expect("claude-sonnet-4.6 should be priced");

        approx_eq(cost, 0.006_435);
    }

    #[test]
    fn leaves_unknown_models_unpriced() {
        assert_eq!(
            estimate_cost_usd("cursor-private-model", TokenBreakdown::default()),
            None
        );
    }
}
