//! Derived Summary aggregation into Reporting Periods.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone};
use rust_decimal::Decimal;

use crate::cost::{CostState, IncompleteCostReason, PricedCodexSession, known_cost};
use crate::session::{DataQualityNotice, TokenTotals};
use crate::usage_source::UsageSourceResolution;

/// Cost-first usage view over all Headline Periods.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedSummary {
    /// Current Source State used for this run.
    pub usage_source_resolution: UsageSourceResolution,
    /// Daily, Weekly, Monthly, and All-Time Headline Periods.
    pub headline_periods: Vec<HeadlinePeriod>,
    /// Month-first All-Time Detail.
    pub all_time_detail: Vec<MonthlySessionGroup>,
    /// Data Quality Detail.
    pub data_quality_notices: Vec<DataQualityNotice>,
}

/// A Headline Period with newest-first Session Detail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeadlinePeriod {
    /// Reporting Period kind.
    pub kind: ReportingPeriodKind,
    /// Local start date when finite.
    pub local_start_date: Option<NaiveDate>,
    /// Local end date when finite.
    pub local_end_date: Option<NaiveDate>,
    /// Aggregated totals and cost state.
    pub summary_totals: SummaryTotals,
    /// Known United States Dollar Cost for the previous matching Reporting Period.
    pub previous_period_known_united_states_dollar_cost: Option<Decimal>,
    /// Newest-first Session Detail.
    pub session_detail: Vec<PricedCodexSession>,
}

/// Reporting Period kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReportingPeriodKind {
    /// Current local day.
    Daily,
    /// Current Monday-start week.
    Weekly,
    /// Current calendar month.
    Monthly,
    /// All known usage.
    AllTime,
}

/// Aggregated token and cost totals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SummaryTotals {
    /// Known United States Dollar Cost.
    pub known_united_states_dollar_cost: Decimal,
    /// Period cost state.
    pub period_cost_state: PeriodCostState,
    /// Aggregated token totals.
    pub token_totals: TokenTotals,
    /// Session count.
    pub session_count: usize,
}

/// Aggregate cost state for a period.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PeriodCostState {
    /// No readable usage source exists.
    MissingUsageSource,
    /// Readable source exists with no sessions in period.
    ZeroUsage,
    /// All sessions are fully priced.
    Complete,
    /// Some known cost and some incomplete usage.
    Partial { reasons: Vec<IncompleteCostReason> },
    /// Usage exists but no cost is known.
    Incomplete { reasons: Vec<IncompleteCostReason> },
}

/// All-Time sessions grouped by local calendar month, newest month first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonthlySessionGroup {
    /// First day of the local calendar month.
    pub local_month_start_date: NaiveDate,
    /// Newest-first Session Detail for the month.
    pub session_detail: Vec<PricedCodexSession>,
}

/// Builds a Derived Summary from the Current Source State.
pub fn build_derived_summary(
    usage_source_resolution: UsageSourceResolution,
    priced_sessions: Vec<PricedCodexSession>,
    data_quality_notices: Vec<DataQualityNotice>,
) -> DerivedSummary {
    build_derived_summary_at(
        usage_source_resolution,
        priced_sessions,
        data_quality_notices,
        Local::now(),
    )
}

/// Builds a Derived Summary using an explicit current local time.
pub fn build_derived_summary_at(
    usage_source_resolution: UsageSourceResolution,
    priced_sessions: Vec<PricedCodexSession>,
    data_quality_notices: Vec<DataQualityNotice>,
    current_local_time: DateTime<Local>,
) -> DerivedSummary {
    let today = current_local_time.date_naive();
    let week_start = today - Duration::days(today.weekday().num_days_from_monday().into());
    let month_start = today.with_day(1).expect("current month has first day");
    let previous_day = today - Duration::days(1);
    let previous_week_start = week_start - Duration::days(7);
    let previous_month_end = month_start - Duration::days(1);
    let previous_month_start = previous_month_end
        .with_day(1)
        .expect("previous month has first day");

    let headline_periods = vec![
        period(
            ReportingPeriodKind::Daily,
            Some(today),
            Some(today),
            Some(previous_day),
            Some(previous_day),
            &usage_source_resolution,
            &priced_sessions,
        ),
        period(
            ReportingPeriodKind::Weekly,
            Some(week_start),
            Some(week_start + Duration::days(6)),
            Some(previous_week_start),
            Some(week_start - Duration::days(1)),
            &usage_source_resolution,
            &priced_sessions,
        ),
        period(
            ReportingPeriodKind::Monthly,
            Some(month_start),
            None,
            Some(previous_month_start),
            Some(previous_month_end),
            &usage_source_resolution,
            &priced_sessions,
        ),
        period(
            ReportingPeriodKind::AllTime,
            None,
            None,
            None,
            None,
            &usage_source_resolution,
            &priced_sessions,
        ),
    ];

    DerivedSummary {
        usage_source_resolution,
        headline_periods,
        all_time_detail: all_time_detail(&priced_sessions),
        data_quality_notices,
    }
}

fn period(
    kind: ReportingPeriodKind,
    local_start_date: Option<NaiveDate>,
    local_end_date: Option<NaiveDate>,
    previous_local_start_date: Option<NaiveDate>,
    previous_local_end_date: Option<NaiveDate>,
    usage_source_resolution: &UsageSourceResolution,
    priced_sessions: &[PricedCodexSession],
) -> HeadlinePeriod {
    let mut session_detail = priced_sessions
        .iter()
        .filter(|priced_session| {
            session_matches_reporting_period(priced_session, local_start_date, local_end_date)
        })
        .cloned()
        .collect::<Vec<_>>();

    sort_newest_first(&mut session_detail);
    let summary_totals = summary_totals(usage_source_resolution, &session_detail);
    let previous_period_known_united_states_dollar_cost =
        previous_period_known_united_states_dollar_cost(
            previous_local_start_date,
            previous_local_end_date,
            usage_source_resolution,
            priced_sessions,
        );

    HeadlinePeriod {
        kind,
        local_start_date,
        local_end_date,
        summary_totals,
        previous_period_known_united_states_dollar_cost,
        session_detail,
    }
}

fn previous_period_known_united_states_dollar_cost(
    previous_local_start_date: Option<NaiveDate>,
    previous_local_end_date: Option<NaiveDate>,
    usage_source_resolution: &UsageSourceResolution,
    priced_sessions: &[PricedCodexSession],
) -> Option<Decimal> {
    if !usage_source_resolution.is_readable() {
        return None;
    }

    let previous_local_start_date = previous_local_start_date?;
    Some(
        priced_sessions
            .iter()
            .filter(|priced_session| {
                session_matches_reporting_period(
                    priced_session,
                    Some(previous_local_start_date),
                    previous_local_end_date,
                )
            })
            .map(|priced_session| known_cost(&priced_session.cost_state))
            .sum(),
    )
}

fn session_matches_reporting_period(
    priced_session: &PricedCodexSession,
    local_start_date: Option<NaiveDate>,
    local_end_date: Option<NaiveDate>,
) -> bool {
    let local_date = priced_session
        .codex_session
        .session_start_time
        .with_timezone(&Local)
        .date_naive();

    match (local_start_date, local_end_date) {
        (Some(start), Some(end)) => local_date >= start && local_date <= end,
        (Some(start), None) => {
            local_date >= start
                && local_date.month() == start.month()
                && local_date.year() == start.year()
        }
        (None, None) => true,
        (None, Some(_)) => true,
    }
}

fn summary_totals(
    usage_source_resolution: &UsageSourceResolution,
    session_detail: &[PricedCodexSession],
) -> SummaryTotals {
    let token_totals =
        session_detail
            .iter()
            .fold(TokenTotals::default(), |mut total, priced_session| {
                add_token_totals(&mut total, &priced_session.codex_session.token_totals);
                total
            });
    let known_united_states_dollar_cost = session_detail
        .iter()
        .map(|priced_session| known_cost(&priced_session.cost_state))
        .sum();
    let reasons = session_detail
        .iter()
        .flat_map(|priced_session| incomplete_reasons(&priced_session.cost_state))
        .collect::<Vec<_>>();
    let period_cost_state = if !usage_source_resolution.is_readable() {
        PeriodCostState::MissingUsageSource
    } else if session_detail.is_empty() {
        PeriodCostState::ZeroUsage
    } else if reasons.is_empty() {
        PeriodCostState::Complete
    } else if known_united_states_dollar_cost > Decimal::ZERO {
        PeriodCostState::Partial { reasons }
    } else {
        PeriodCostState::Incomplete { reasons }
    };

    SummaryTotals {
        known_united_states_dollar_cost,
        period_cost_state,
        token_totals,
        session_count: session_detail.len(),
    }
}

fn incomplete_reasons(cost_state: &CostState) -> Vec<IncompleteCostReason> {
    match cost_state {
        CostState::Complete { .. } => Vec::new(),
        CostState::Partial { reasons, .. } | CostState::Incomplete { reasons } => reasons.clone(),
    }
}

fn add_token_totals(total: &mut TokenTotals, current: &TokenTotals) {
    total.input_tokens = add_optional(total.input_tokens, current.input_tokens);
    total.non_cached_input_tokens = add_optional(
        total.non_cached_input_tokens,
        current.non_cached_input_tokens,
    );
    total.cache_read_tokens = add_optional(total.cache_read_tokens, current.cache_read_tokens);
    total.cache_write_tokens = add_optional(total.cache_write_tokens, current.cache_write_tokens);
    total.output_tokens = add_optional(total.output_tokens, current.output_tokens);
    total.reasoning_output_tokens = add_optional(
        total.reasoning_output_tokens,
        current.reasoning_output_tokens,
    );
    total.total_tokens = add_optional(total.total_tokens, current.total_tokens);
}

fn add_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match right {
        Some(right) => Some(left.unwrap_or(0) + right),
        None => left,
    }
}

fn all_time_detail(priced_sessions: &[PricedCodexSession]) -> Vec<MonthlySessionGroup> {
    let mut grouped_sessions: BTreeMap<NaiveDate, Vec<PricedCodexSession>> = BTreeMap::new();
    for priced_session in priced_sessions {
        let local_date = priced_session
            .codex_session
            .session_start_time
            .with_timezone(&Local)
            .date_naive();
        let month_start = Local
            .with_ymd_and_hms(local_date.year(), local_date.month(), 1, 0, 0, 0)
            .unwrap()
            .date_naive();
        grouped_sessions
            .entry(month_start)
            .or_default()
            .push(priced_session.clone());
    }

    grouped_sessions
        .into_iter()
        .rev()
        .map(|(local_month_start_date, mut session_detail)| {
            sort_newest_first(&mut session_detail);
            MonthlySessionGroup {
                local_month_start_date,
                session_detail,
            }
        })
        .collect()
}

fn sort_newest_first(session_detail: &mut [PricedCodexSession]) {
    session_detail.sort_by_key(|priced_session| {
        std::cmp::Reverse(priced_session.codex_session.session_detail_time())
    });
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::cost::CostState;
    use crate::session::{AiCodingAgent, CodexSession, TokenTotals};

    #[test]
    fn produces_headline_periods_with_local_calendar_membership() {
        let current_local_time = Local.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap();
        let sessions = vec![
            priced_session(
                "late",
                Local
                    .with_ymd_and_hms(2026, 5, 10, 23, 30, 0)
                    .unwrap()
                    .with_timezone(&Utc),
                20,
            ),
            priced_session(
                "week",
                Local
                    .with_ymd_and_hms(2026, 5, 4, 1, 0, 0)
                    .unwrap()
                    .with_timezone(&Utc),
                10,
            ),
            priced_session(
                "month",
                Local
                    .with_ymd_and_hms(2026, 5, 1, 1, 0, 0)
                    .unwrap()
                    .with_timezone(&Utc),
                5,
            ),
            priced_session(
                "old",
                Local
                    .with_ymd_and_hms(2026, 4, 30, 23, 0, 0)
                    .unwrap()
                    .with_timezone(&Utc),
                1,
            ),
        ];

        let derived_summary = build_derived_summary_at(
            UsageSourceResolution::Readable {
                path: "source".into(),
                is_custom: false,
            },
            sessions,
            Vec::new(),
            current_local_time,
        );

        assert_eq!(derived_summary.headline_periods.len(), 4);
        assert_eq!(
            derived_summary.headline_periods[0].kind,
            ReportingPeriodKind::Daily
        );
        assert_eq!(
            derived_summary.headline_periods[0]
                .summary_totals
                .session_count,
            1
        );
        assert_eq!(
            derived_summary.headline_periods[0].previous_period_known_united_states_dollar_cost,
            Some(Decimal::ZERO)
        );
        assert_eq!(
            derived_summary.headline_periods[1].local_start_date,
            Some(NaiveDate::from_ymd_opt(2026, 5, 4).unwrap())
        );
        assert_eq!(
            derived_summary.headline_periods[1]
                .summary_totals
                .session_count,
            2
        );
        assert_eq!(
            derived_summary.headline_periods[1].previous_period_known_united_states_dollar_cost,
            Some(Decimal::from(6))
        );
        assert_eq!(
            derived_summary.headline_periods[2]
                .summary_totals
                .session_count,
            3
        );
        assert_eq!(
            derived_summary.headline_periods[2].previous_period_known_united_states_dollar_cost,
            Some(Decimal::from(1))
        );
        assert_eq!(
            derived_summary.headline_periods[3]
                .summary_totals
                .session_count,
            4
        );
        assert_eq!(
            derived_summary.headline_periods[3].previous_period_known_united_states_dollar_cost,
            None
        );
        assert_eq!(
            derived_summary.headline_periods[3].session_detail[0]
                .codex_session
                .project_name
                .as_deref(),
            Some("late")
        );
        assert_eq!(
            derived_summary.all_time_detail[0].local_month_start_date,
            NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()
        );
    }

    #[test]
    fn empty_readable_period_is_zero_usage_and_missing_source_is_missing() {
        let current_local_time = Local.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap();
        let empty_summary = build_derived_summary_at(
            UsageSourceResolution::Readable {
                path: "source".into(),
                is_custom: false,
            },
            Vec::new(),
            Vec::new(),
            current_local_time,
        );
        let missing_summary = build_derived_summary_at(
            UsageSourceResolution::Missing {
                path: "source".into(),
                is_custom: false,
            },
            Vec::new(),
            Vec::new(),
            current_local_time,
        );

        assert_eq!(
            empty_summary.headline_periods[0]
                .summary_totals
                .period_cost_state,
            PeriodCostState::ZeroUsage
        );
        assert_eq!(
            missing_summary.headline_periods[0]
                .summary_totals
                .period_cost_state,
            PeriodCostState::MissingUsageSource
        );
    }

    #[test]
    fn session_detail_sorts_by_display_time() {
        let current_local_time = Local.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap();
        let mut session_with_older_display_time = priced_session(
            "older-display-time",
            Local
                .with_ymd_and_hms(2026, 5, 10, 10, 5, 0)
                .unwrap()
                .with_timezone(&Utc),
            10,
        );
        session_with_older_display_time
            .codex_session
            .session_last_modified_time = Some(
            Local
                .with_ymd_and_hms(2026, 5, 10, 10, 20, 0)
                .unwrap()
                .with_timezone(&Utc),
        );
        let mut session_with_newer_display_time = priced_session(
            "newer-display-time",
            Local
                .with_ymd_and_hms(2026, 5, 10, 10, 0, 0)
                .unwrap()
                .with_timezone(&Utc),
            20,
        );
        session_with_newer_display_time
            .codex_session
            .session_last_modified_time = Some(
            Local
                .with_ymd_and_hms(2026, 5, 10, 10, 22, 0)
                .unwrap()
                .with_timezone(&Utc),
        );

        let derived_summary = build_derived_summary_at(
            UsageSourceResolution::Readable {
                path: "source".into(),
                is_custom: false,
            },
            vec![
                session_with_older_display_time,
                session_with_newer_display_time,
            ],
            Vec::new(),
            current_local_time,
        );

        assert_eq!(
            derived_summary.headline_periods[3].session_detail[0]
                .codex_session
                .project_name
                .as_deref(),
            Some("newer-display-time")
        );
    }

    fn priced_session(
        project_name: &str,
        session_start_time: DateTime<Utc>,
        output_tokens: u64,
    ) -> PricedCodexSession {
        PricedCodexSession {
            codex_session: CodexSession {
                ai_coding_agent: AiCodingAgent::Codex,
                source_path: format!("{project_name}.jsonl").into(),
                session_name: Some(format!("{project_name} session")),
                session_start_time,
                session_last_modified_time: None,
                model: "gpt-5.5".to_owned(),
                reasoning_effort: None,
                project_path: Some(format!("/tmp/{project_name}").into()),
                project_name: Some(project_name.to_owned()),
                is_active: false,
                token_totals: TokenTotals {
                    input_tokens: Some(10),
                    output_tokens: Some(output_tokens),
                    total_tokens: Some(10 + output_tokens),
                    ..TokenTotals::default()
                },
            },
            cost_state: CostState::Complete {
                united_states_dollar_cost: Decimal::from(output_tokens),
            },
            price_schedule_match: Some(crate::cost::PriceScheduleMatch {
                model: "gpt-5.5".to_owned(),
                effective_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                input_tokens_per_million: Decimal::from_str_exact("5.000").unwrap(),
                cache_read_tokens_per_million: Decimal::from_str_exact("0.500").unwrap(),
                cache_write_tokens_per_million: Decimal::from_str_exact("5.000").unwrap(),
                output_tokens_per_million: Decimal::from_str_exact("30.000").unwrap(),
                reasoning_output_tokens_per_million: None,
            }),
        }
    }
}
