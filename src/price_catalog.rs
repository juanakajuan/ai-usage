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
                    "2026-05-10",
                    "5.000",
                    "0.500",
                    "5.000",
                    "30.000",
                    None,
                ),
                schedule(
                    "gpt-5.5-fast",
                    "2026-05-10",
                    "12.500",
                    "1.250",
                    "12.500",
                    "75.000",
                    None,
                ),
                schedule(
                    "gpt-5.4",
                    "2026-05-10",
                    "2.500",
                    "0.250",
                    "2.500",
                    "15.000",
                    None,
                ),
                schedule(
                    "gpt-5.4-mini",
                    "2026-05-10",
                    "0.750",
                    "0.075",
                    "0.750",
                    "4.500",
                    None,
                ),
                schedule(
                    "gpt-5.4-nano",
                    "2026-05-10",
                    "0.200",
                    "0.020",
                    "0.200",
                    "1.250",
                    None,
                ),
                schedule(
                    "gpt-5.3-codex",
                    "2026-05-10",
                    "1.750",
                    "0.175",
                    "1.750",
                    "14.000",
                    None,
                ),
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
                    "gpt-5.5-fast",
                    "2026-01-01",
                    "3.125",
                    "0.3125",
                    "3.125",
                    "25.000",
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
    fn current_standard_prices_match_bundled_flagship_catalog() {
        let price_catalog = PriceCatalog::bundled();
        let price_schedule = price_catalog
            .effective_price_schedule(
                "gpt-5.5",
                Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
            )
            .expect("gpt-5.5 price schedule");

        assert_eq!(price_schedule.effective_date, date(2026, 5, 10));
        assert_eq!(
            price_schedule.input_tokens_per_million,
            Decimal::from_str_exact("5.000").unwrap()
        );
        assert_eq!(
            price_schedule.cache_read_tokens_per_million,
            Decimal::from_str_exact("0.500").unwrap()
        );
        assert_eq!(
            price_schedule.output_tokens_per_million,
            Decimal::from_str_exact("30.000").unwrap()
        );

        let mini_price_schedule = price_catalog
            .effective_price_schedule(
                "gpt-5.4-mini",
                Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
            )
            .expect("gpt-5.4-mini price schedule");
        assert_eq!(
            mini_price_schedule.cache_read_tokens_per_million,
            Decimal::from_str_exact("0.075").unwrap()
        );
    }

    #[test]
    fn fast_mode_prices_are_two_and_a_half_times_standard_model_prices() {
        let price_catalog = PriceCatalog::bundled();
        let standard_price_schedule = price_catalog
            .effective_price_schedule(
                "gpt-5.5",
                Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
            )
            .expect("gpt-5.5 price schedule");
        let fast_price_schedule = price_catalog
            .effective_price_schedule(
                "gpt-5.5-fast",
                Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
            )
            .expect("gpt-5.5-fast price schedule");
        let fast_multiplier = Decimal::from_str_exact("2.5").unwrap();

        assert_eq!(
            fast_price_schedule.input_tokens_per_million,
            standard_price_schedule.input_tokens_per_million * fast_multiplier
        );
        assert_eq!(
            fast_price_schedule.cache_read_tokens_per_million,
            standard_price_schedule.cache_read_tokens_per_million * fast_multiplier
        );
        assert_eq!(
            fast_price_schedule.cache_write_tokens_per_million,
            standard_price_schedule.cache_write_tokens_per_million * fast_multiplier
        );
        assert_eq!(
            fast_price_schedule.output_tokens_per_million,
            standard_price_schedule.output_tokens_per_million * fast_multiplier
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

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("valid test date")
    }
}
