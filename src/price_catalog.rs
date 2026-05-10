//! Bundled offline Price Catalog and Effective Price Schedule resolution.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

/// Bundled local Price Catalog.
#[derive(Clone, Debug)]
pub struct PriceCatalog {
    price_schedules: Vec<PriceSchedule>,
}

/// A dated model price schedule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceSchedule {
    /// Exact Model name this schedule prices.
    pub model: String,
    /// First date on which this schedule is effective.
    pub effective_date: NaiveDate,
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

impl PriceCatalog {
    /// Returns the bundled offline Price Catalog.
    pub fn bundled() -> Self {
        Self {
            price_schedules: vec![
                schedule(
                    "gpt-5.5",
                    "2026-01-01",
                    "1.250",
                    "0.125",
                    "1.250",
                    "10.000",
                    None,
                ),
                schedule(
                    "gpt-5.4",
                    "2026-01-01",
                    "1.250",
                    "0.125",
                    "1.250",
                    "10.000",
                    None,
                ),
                schedule(
                    "gpt-5.4-mini",
                    "2026-01-01",
                    "0.250",
                    "0.025",
                    "0.250",
                    "2.000",
                    None,
                ),
                schedule(
                    "gpt-5.3-codex",
                    "2026-01-01",
                    "1.250",
                    "0.125",
                    "1.250",
                    "10.000",
                    None,
                ),
                schedule(
                    "gpt-5.3-codex-spark",
                    "2026-01-01",
                    "0.250",
                    "0.025",
                    "0.250",
                    "2.000",
                    None,
                ),
                schedule(
                    "gpt-5.2",
                    "2026-01-01",
                    "1.250",
                    "0.125",
                    "1.250",
                    "10.000",
                    None,
                ),
            ],
        }
    }

    /// Resolves the Effective Price Schedule for an exact Model and Session Start Time.
    pub fn effective_price_schedule(
        &self,
        model: &str,
        session_start_time: DateTime<Utc>,
    ) -> Option<&PriceSchedule> {
        let session_date = session_start_time.date_naive();
        self.price_schedules
            .iter()
            .filter(|price_schedule| price_schedule.model == model)
            .filter(|price_schedule| price_schedule.effective_date <= session_date)
            .max_by_key(|price_schedule| price_schedule.effective_date)
    }
}

fn schedule(
    model: &str,
    effective_date: &str,
    input_tokens_per_million: &str,
    cache_read_tokens_per_million: &str,
    cache_write_tokens_per_million: &str,
    output_tokens_per_million: &str,
    reasoning_output_tokens_per_million: Option<&str>,
) -> PriceSchedule {
    PriceSchedule {
        model: model.to_owned(),
        effective_date: NaiveDate::parse_from_str(effective_date, "%Y-%m-%d")
            .expect("valid bundled date"),
        input_tokens_per_million: Decimal::from_str_exact(input_tokens_per_million)
            .expect("valid bundled price"),
        cache_read_tokens_per_million: Decimal::from_str_exact(cache_read_tokens_per_million)
            .expect("valid bundled price"),
        cache_write_tokens_per_million: Decimal::from_str_exact(cache_write_tokens_per_million)
            .expect("valid bundled price"),
        output_tokens_per_million: Decimal::from_str_exact(output_tokens_per_million)
            .expect("valid bundled price"),
        reasoning_output_tokens_per_million: reasoning_output_tokens_per_million
            .map(|price| Decimal::from_str_exact(price).expect("valid bundled price")),
    }
}

/// Converts tokens and a per-million-token price to United States dollar cost.
pub fn cost_for_tokens(token_count: u64, price_per_million_tokens: Decimal) -> Decimal {
    Decimal::from_u64(token_count).expect("token count fits Decimal") * price_per_million_tokens
        / Decimal::from(1_000_000u64)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn exact_model_matching_prices_known_models() {
        let price_catalog = PriceCatalog::bundled();
        assert!(
            price_catalog
                .effective_price_schedule(
                    "gpt-5.5",
                    Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap()
                )
                .is_some()
        );
    }

    #[test]
    fn unknown_models_are_not_guessed() {
        let price_catalog = PriceCatalog::bundled();
        assert!(
            price_catalog
                .effective_price_schedule(
                    "gpt-5",
                    Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap()
                )
                .is_none()
        );
    }

    #[test]
    fn effective_date_boundary_is_inclusive() {
        let price_catalog = PriceCatalog::bundled();
        assert!(
            price_catalog
                .effective_price_schedule(
                    "gpt-5.5",
                    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
                )
                .is_some()
        );
        assert!(
            price_catalog
                .effective_price_schedule(
                    "gpt-5.5",
                    Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap()
                )
                .is_none()
        );
    }
}
