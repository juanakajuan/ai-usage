//! Historical Cost calculation and incomplete cost states.

use rust_decimal::Decimal;

use crate::price_catalog::{PriceCatalog, cost_for_tokens};
use crate::session::{CodexSession, TokenTotals};

/// A Codex Session with cost state attached.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PricedCodexSession {
    /// Source Codex Session.
    pub codex_session: CodexSession,
    /// Historical Cost state.
    pub cost_state: CostState,
    /// Effective Price Schedule match when one was found.
    pub price_schedule_match: Option<PriceScheduleMatch>,
}

/// The Effective Price Schedule selected for a session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceScheduleMatch {
    /// Exact Model name matched by the Price Schedule.
    pub model: String,
    /// Effective date for the matched Price Schedule.
    pub effective_date: chrono::NaiveDate,
    /// Input Tokens price per one million tokens.
    pub input_tokens_per_million: Decimal,
    /// Cache Read Tokens price per one million tokens.
    pub cache_read_tokens_per_million: Decimal,
    /// Cache Write Tokens price per one million tokens.
    pub cache_write_tokens_per_million: Decimal,
    /// Output Tokens price per one million tokens.
    pub output_tokens_per_million: Decimal,
    /// Reasoning Output Tokens price per one million tokens, when distinct.
    pub reasoning_output_tokens_per_million: Option<Decimal>,
}

/// Typed Historical Cost state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CostState {
    /// Complete Historical Cost.
    Complete {
        /// United States Dollar Cost with full calculation precision.
        united_states_dollar_cost: Decimal,
    },
    /// Known Partial Cost with explicit incomplete reasons.
    Partial {
        /// Known United States Dollar Cost.
        known_united_states_dollar_cost: Decimal,
        /// Incomplete reasons.
        reasons: Vec<IncompleteCostReason>,
    },
    /// No cost can be calculated.
    Incomplete {
        /// Incomplete reasons.
        reasons: Vec<IncompleteCostReason>,
    },
}

/// Explicit reason a usage record cannot be fully priced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IncompleteCostReason {
    /// Model has no unambiguous Effective Price Schedule.
    UnpricedUsage { model: String },
    /// Required token category is missing.
    MissingTokenCategory { token_category: String },
}

/// Prices all sessions from the bundled Price Catalog.
pub fn price_sessions(
    codex_sessions: Vec<CodexSession>,
    price_catalog: &PriceCatalog,
) -> Vec<PricedCodexSession> {
    codex_sessions
        .into_iter()
        .map(|codex_session| {
            let price_schedule_match = price_catalog
                .effective_price_schedule(&codex_session.model, codex_session.session_start_time)
                .map(|price_schedule| PriceScheduleMatch {
                    model: price_schedule.model.clone(),
                    effective_date: price_schedule.effective_date,
                    input_tokens_per_million: price_schedule.input_tokens_per_million,
                    cache_read_tokens_per_million: price_schedule.cache_read_tokens_per_million,
                    cache_write_tokens_per_million: price_schedule.cache_write_tokens_per_million,
                    output_tokens_per_million: price_schedule.output_tokens_per_million,
                    reasoning_output_tokens_per_million: price_schedule
                        .reasoning_output_tokens_per_million,
                });
            let cost_state = price_session(&codex_session, price_catalog);
            PricedCodexSession {
                codex_session,
                cost_state,
                price_schedule_match,
            }
        })
        .collect()
}

/// Calculates Historical Cost for a Codex Session.
pub fn price_session(codex_session: &CodexSession, price_catalog: &PriceCatalog) -> CostState {
    let Some(price_schedule) = price_catalog
        .effective_price_schedule(&codex_session.model, codex_session.session_start_time)
    else {
        return CostState::Incomplete {
            reasons: vec![IncompleteCostReason::UnpricedUsage {
                model: codex_session.model.clone(),
            }],
        };
    };

    let TokenTotals {
        input_tokens,
        non_cached_input_tokens,
        cache_read_tokens,
        cache_write_tokens,
        output_tokens,
        reasoning_output_tokens,
        ..
    } = &codex_session.token_totals;

    let mut reasons = Vec::new();
    let mut cost = Decimal::ZERO;

    let non_cached_input_tokens = match (non_cached_input_tokens, input_tokens, cache_read_tokens) {
        (Some(non_cached_input_tokens), _, _) => Some(*non_cached_input_tokens),
        (None, Some(input_tokens), None) => Some(*input_tokens),
        (None, Some(input_tokens), Some(cache_read_tokens))
            if input_tokens >= cache_read_tokens =>
        {
            Some(input_tokens - cache_read_tokens)
        }
        _ => None,
    };

    if let Some(non_cached_input_tokens) = non_cached_input_tokens {
        let cache_write_tokens = cache_write_tokens.unwrap_or(0);
        if cache_write_tokens <= non_cached_input_tokens {
            cost += cost_for_tokens(
                non_cached_input_tokens - cache_write_tokens,
                price_schedule.input_tokens_per_million,
            );
            cost += cost_for_tokens(
                cache_write_tokens,
                price_schedule.cache_write_tokens_per_million,
            );
        } else {
            reasons.push(IncompleteCostReason::MissingTokenCategory {
                token_category: "Non-Cached Input Tokens".to_owned(),
            });
        }
    } else {
        if let Some(cache_write_tokens) = cache_write_tokens {
            cost += cost_for_tokens(
                *cache_write_tokens,
                price_schedule.cache_write_tokens_per_million,
            );
        }
        reasons.push(IncompleteCostReason::MissingTokenCategory {
            token_category: "Non-Cached Input Tokens".to_owned(),
        });
    }

    if let Some(cache_read_tokens) = cache_read_tokens {
        cost += cost_for_tokens(
            *cache_read_tokens,
            price_schedule.cache_read_tokens_per_million,
        );
    }

    match (output_tokens, reasoning_output_tokens) {
        (Some(output_tokens), Some(reasoning_output_tokens))
            if price_schedule.reasoning_output_tokens_per_million.is_some()
                && reasoning_output_tokens <= output_tokens =>
        {
            cost += cost_for_tokens(
                output_tokens - reasoning_output_tokens,
                price_schedule.output_tokens_per_million,
            );
            cost += cost_for_tokens(
                *reasoning_output_tokens,
                price_schedule
                    .reasoning_output_tokens_per_million
                    .expect("checked distinct reasoning output price"),
            );
        }
        (Some(output_tokens), Some(reasoning_output_tokens))
            if price_schedule.reasoning_output_tokens_per_million.is_some()
                && reasoning_output_tokens > output_tokens =>
        {
            cost += cost_for_tokens(*output_tokens, price_schedule.output_tokens_per_million);
            reasons.push(IncompleteCostReason::MissingTokenCategory {
                token_category: "Output Tokens".to_owned(),
            });
        }
        (Some(output_tokens), _) => {
            cost += cost_for_tokens(*output_tokens, price_schedule.output_tokens_per_million);
        }
        (None, Some(reasoning_output_tokens)) => {
            cost += cost_for_tokens(
                *reasoning_output_tokens,
                price_schedule
                    .reasoning_output_tokens_per_million
                    .unwrap_or(price_schedule.output_tokens_per_million),
            );
            reasons.push(IncompleteCostReason::MissingTokenCategory {
                token_category: "Output Tokens".to_owned(),
            });
        }
        (None, None) => {
            reasons.push(IncompleteCostReason::MissingTokenCategory {
                token_category: "Output Tokens".to_owned(),
            });
        }
    }

    if reasons.is_empty() {
        CostState::Complete {
            united_states_dollar_cost: cost,
        }
    } else if cost > Decimal::ZERO {
        CostState::Partial {
            known_united_states_dollar_cost: cost,
            reasons,
        }
    } else {
        CostState::Incomplete { reasons }
    }
}

/// Formats United States Dollar Cost for display only.
pub fn format_united_states_dollar_cost(cost: Decimal) -> String {
    format!("${:.2}", cost.round_dp(2))
}

/// Extracts known cost from complete or partial cost states.
pub fn known_cost(cost_state: &CostState) -> Decimal {
    match cost_state {
        CostState::Complete {
            united_states_dollar_cost,
        } => *united_states_dollar_cost,
        CostState::Partial {
            known_united_states_dollar_cost,
            ..
        } => *known_united_states_dollar_cost,
        CostState::Incomplete { .. } => Decimal::ZERO,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::session::{CodexSession, TokenTotals};

    #[test]
    fn calculates_united_states_dollar_cost_without_double_counting_subtotals() {
        let codex_session = test_session(TokenTotals {
            input_tokens: Some(1_000_000),
            non_cached_input_tokens: Some(800_000),
            cache_read_tokens: Some(200_000),
            cache_write_tokens: Some(100_000),
            output_tokens: Some(500_000),
            reasoning_output_tokens: Some(100_000),
            total_tokens: Some(1_500_000),
        });

        let cost_state = price_session(&codex_session, &PriceCatalog::bundled());

        assert_eq!(
            cost_state,
            CostState::Complete {
                united_states_dollar_cost: Decimal::from_str_exact("19.100").unwrap()
            }
        );
        assert_eq!(
            format_united_states_dollar_cost(Decimal::from_str_exact("7.155").unwrap()),
            "$7.16"
        );
    }

    #[test]
    fn unknown_model_creates_unpriced_incomplete_state() {
        let mut codex_session = test_session(TokenTotals {
            input_tokens: Some(10),
            output_tokens: Some(10),
            ..TokenTotals::default()
        });
        codex_session.model = "unknown-model".to_owned();

        assert_eq!(
            price_session(&codex_session, &PriceCatalog::bundled()),
            CostState::Incomplete {
                reasons: vec![IncompleteCostReason::UnpricedUsage {
                    model: "unknown-model".to_owned()
                }]
            }
        );
    }

    #[test]
    fn missing_output_tokens_create_partial_cost_when_input_cost_is_known() {
        let codex_session = test_session(TokenTotals {
            input_tokens: Some(1_000_000),
            non_cached_input_tokens: Some(1_000_000),
            ..TokenTotals::default()
        });

        assert_eq!(
            price_session(&codex_session, &PriceCatalog::bundled()),
            CostState::Partial {
                known_united_states_dollar_cost: Decimal::from_str_exact("5.000").unwrap(),
                reasons: vec![IncompleteCostReason::MissingTokenCategory {
                    token_category: "Output Tokens".to_owned()
                }]
            }
        );
    }

    fn test_session(token_totals: TokenTotals) -> CodexSession {
        CodexSession {
            source_path: "session.jsonl".into(),
            session_start_time: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 10, 0, 0).unwrap(),
            model: "gpt-5.5".to_owned(),
            project_path: None,
            project_name: None,
            is_active: false,
            token_totals,
        }
    }
}
