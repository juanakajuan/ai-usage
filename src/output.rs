//! JSON Output for Derived Summaries.

use std::env;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cost::{CostState, IncompleteCostReason};
use crate::reporting_period::{
    DerivedSummary, HeadlinePeriod, PeriodCostState, ReportingPeriodKind,
};
use crate::session::{DataQualityNoticeKind, TokenTotals};
use crate::usage_source::CurrentSourceState;

/// Current JSON Output schema version.
pub const OUTPUT_SCHEMA_VERSION: u32 = 3;

/// JSON Output rendering options.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonOutputOptions {
    /// Redact home-directory-sensitive prefixes.
    pub redact_paths: bool,
}

/// Machine-readable Derived Summary output.
#[derive(Clone, Debug, Serialize)]
pub struct JsonOutput {
    /// Output Schema Version.
    pub output_schema_version: u32,
    /// Usage Source details.
    pub usage_source: JsonUsageSource,
    /// Headline Periods.
    pub headline_periods: Vec<JsonHeadlinePeriod>,
    /// Month-first All-Time Detail.
    pub all_time_detail: Vec<JsonMonthlySessionGroup>,
    /// Data Quality Detail.
    pub data_quality_detail: Vec<JsonDataQualityNotice>,
}

#[derive(Clone, Debug, Serialize)]
pub struct JsonUsageSource {
    /// Usage Source path, redacted when requested.
    pub path: String,
    /// Whether the Usage Source is readable.
    pub is_readable: bool,
    /// Whether the Usage Source came from `--usage-source`.
    pub is_custom: bool,
}

/// JSON representation of one Headline Period.
#[derive(Clone, Debug, Serialize)]
pub struct JsonHeadlinePeriod {
    /// Reporting Period kind.
    pub kind: String,
    /// Local start date for finite periods.
    pub local_start_date: Option<String>,
    /// Local end date for finite periods.
    pub local_end_date: Option<String>,
    /// Known United States Dollar Cost with calculation precision.
    pub known_united_states_dollar_cost: String,
    /// Previous matching Reporting Period known United States Dollar Cost.
    pub previous_period_known_united_states_dollar_cost: Option<String>,
    /// Change in known United States Dollar Cost from the previous matching Reporting Period.
    pub known_united_states_dollar_cost_change_from_previous_period: Option<String>,
    /// Aggregate cost state.
    pub period_cost_state: JsonPeriodCostState,
    /// Aggregated token totals.
    pub token_totals: TokenTotals,
    /// Number of sessions in the period.
    pub session_count: usize,
    /// Newest-first Session Detail.
    pub session_detail: Vec<JsonSessionDetail>,
}

/// JSON representation of aggregate period cost state.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JsonPeriodCostState {
    /// No readable Usage Source exists.
    MissingUsageSource,
    /// Readable Usage Source contains no usage for the period.
    ZeroUsage,
    /// All sessions are fully priced.
    Complete,
    /// Some known cost and explicit incomplete reasons.
    Partial {
        /// Incomplete reasons.
        reasons: Vec<JsonIncompleteCostReason>,
    },
    /// No known cost and explicit incomplete reasons.
    Incomplete {
        /// Incomplete reasons.
        reasons: Vec<JsonIncompleteCostReason>,
    },
}

/// JSON representation of one AI Coding Agent Session in Session Detail.
#[derive(Clone, Debug, Serialize)]
pub struct JsonSessionDetail {
    /// AI Coding Agent that produced the session.
    pub ai_coding_agent: String,
    /// Source session file path.
    pub source_path: String,
    /// Source-recorded Session Name when available.
    pub session_name: Option<String>,
    /// Session Start Time.
    pub session_start_time: String,
    /// Exact recorded Model name.
    pub model: String,
    /// Source-recorded reasoning effort or thinking level when available.
    pub reasoning_effort: Option<String>,
    /// Full project path when available.
    pub project_path: Option<String>,
    /// Compact Project Name when available.
    pub project_name: Option<String>,
    /// Whether the session is active.
    pub is_active: bool,
    /// Session token totals.
    pub token_totals: TokenTotals,
    /// Session Historical Cost state.
    pub cost_state: JsonCostState,
}

/// JSON representation of session Historical Cost state.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JsonCostState {
    /// Fully priced Historical Cost.
    Complete {
        /// United States Dollar Cost with calculation precision.
        united_states_dollar_cost: String,
    },
    /// Partial known cost with incomplete reasons.
    Partial {
        /// Known United States Dollar Cost with calculation precision.
        known_united_states_dollar_cost: String,
        /// Incomplete reasons.
        reasons: Vec<JsonIncompleteCostReason>,
    },
    /// Incomplete cost with no known cost.
    Incomplete {
        /// Incomplete reasons.
        reasons: Vec<JsonIncompleteCostReason>,
    },
}

/// JSON representation of an incomplete cost reason.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JsonIncompleteCostReason {
    /// Model has no Effective Price Schedule.
    UnpricedUsage { model: String },
    /// A required token category is missing.
    MissingTokenCategory { token_category: String },
}

/// JSON representation of month-first All-Time Detail.
#[derive(Clone, Debug, Serialize)]
pub struct JsonMonthlySessionGroup {
    /// First day of the local calendar month.
    pub local_month_start_date: String,
    /// Newest-first Session Detail for this month.
    pub session_detail: Vec<JsonSessionDetail>,
}

/// JSON representation of a Data Quality Notice.
#[derive(Clone, Debug, Serialize)]
pub struct JsonDataQualityNotice {
    /// Source path associated with the notice.
    pub source_path: String,
    /// One-based line number when applicable.
    pub line_number: Option<usize>,
    /// Stable notice kind.
    pub kind: String,
    /// Human-readable detail.
    pub detail: String,
}

/// Builds JSON Output from the same Derived Summary used by terminal presentation.
pub fn build_json_output(
    derived_summary: &DerivedSummary,
    options: JsonOutputOptions,
) -> Result<JsonOutput, std::io::Error> {
    let home_directory = env::var_os("HOME").map(PathBuf::from);

    Ok(JsonOutput {
        output_schema_version: OUTPUT_SCHEMA_VERSION,
        usage_source: usage_source_json(
            &derived_summary.current_source_state,
            options.redact_paths,
            home_directory.as_deref(),
        ),
        headline_periods: derived_summary
            .headline_periods
            .iter()
            .map(|headline_period| JsonHeadlinePeriod {
                kind: reporting_period_kind_json(&headline_period.kind).to_owned(),
                local_start_date: headline_period
                    .local_start_date
                    .map(|date| date.to_string()),
                local_end_date: headline_period.local_end_date.map(|date| date.to_string()),
                known_united_states_dollar_cost: headline_period
                    .summary_totals
                    .known_united_states_dollar_cost
                    .to_string(),
                previous_period_known_united_states_dollar_cost: headline_period
                    .previous_period_known_united_states_dollar_cost
                    .map(|cost| cost.to_string()),
                known_united_states_dollar_cost_change_from_previous_period:
                    known_united_states_dollar_cost_change_from_previous_period_json(
                        headline_period,
                    ),
                period_cost_state: period_cost_state_json(
                    &headline_period.summary_totals.period_cost_state,
                ),
                token_totals: headline_period.summary_totals.token_totals.clone(),
                session_count: headline_period.summary_totals.session_count,
                session_detail: headline_period
                    .session_detail
                    .iter()
                    .map(|priced_session| {
                        session_detail_json(
                            priced_session,
                            options.redact_paths,
                            home_directory.as_deref(),
                        )
                    })
                    .collect(),
            })
            .collect(),
        all_time_detail: derived_summary
            .all_time_detail
            .iter()
            .map(|monthly_session_group| JsonMonthlySessionGroup {
                local_month_start_date: monthly_session_group.local_month_start_date.to_string(),
                session_detail: monthly_session_group
                    .session_detail
                    .iter()
                    .map(|priced_session| {
                        session_detail_json(
                            priced_session,
                            options.redact_paths,
                            home_directory.as_deref(),
                        )
                    })
                    .collect(),
            })
            .collect(),
        data_quality_detail: derived_summary
            .data_quality_notices
            .iter()
            .map(|notice| JsonDataQualityNotice {
                source_path: path_json(
                    &notice.source_path,
                    options.redact_paths,
                    home_directory.as_deref(),
                ),
                line_number: notice.line_number,
                kind: data_quality_notice_kind_json(&notice.kind).to_owned(),
                detail: notice.detail.clone(),
            })
            .collect(),
    })
}

fn known_united_states_dollar_cost_change_from_previous_period_json(
    headline_period: &HeadlinePeriod,
) -> Option<String> {
    headline_period
        .previous_period_known_united_states_dollar_cost
        .map(|previous_period_known_united_states_dollar_cost| {
            (headline_period
                .summary_totals
                .known_united_states_dollar_cost
                - previous_period_known_united_states_dollar_cost)
                .to_string()
        })
}

fn usage_source_json(
    current_source_state: &CurrentSourceState,
    redact_paths: bool,
    home_directory: Option<&Path>,
) -> JsonUsageSource {
    match current_source_state {
        CurrentSourceState::Readable { path, is_custom } => JsonUsageSource {
            path: path_json(path, redact_paths, home_directory),
            is_readable: true,
            is_custom: *is_custom,
        },
        CurrentSourceState::Missing { path, is_custom } => JsonUsageSource {
            path: path_json(path, redact_paths, home_directory),
            is_readable: false,
            is_custom: *is_custom,
        },
    }
}

fn session_detail_json(
    priced_session: &crate::cost::PricedCodexSession,
    redact_paths: bool,
    home_directory: Option<&Path>,
) -> JsonSessionDetail {
    JsonSessionDetail {
        ai_coding_agent: priced_session
            .codex_session
            .ai_coding_agent
            .label()
            .to_owned(),
        source_path: path_json(
            &priced_session.codex_session.source_path,
            redact_paths,
            home_directory,
        ),
        session_name: priced_session.codex_session.session_name.clone(),
        session_start_time: priced_session.codex_session.session_start_time.to_rfc3339(),
        model: priced_session.codex_session.model.clone(),
        reasoning_effort: priced_session.codex_session.reasoning_effort.clone(),
        project_path: priced_session
            .codex_session
            .project_path
            .as_ref()
            .map(|path| path_json(path, redact_paths, home_directory)),
        project_name: priced_session.codex_session.project_name.clone(),
        is_active: priced_session.codex_session.is_active,
        token_totals: priced_session.codex_session.token_totals.clone(),
        cost_state: cost_state_json(&priced_session.cost_state),
    }
}

fn path_json(path: &Path, redact_paths: bool, home_directory: Option<&Path>) -> String {
    if redact_paths
        && let Some(home_directory) = home_directory
        && let Ok(relative_path) = path.strip_prefix(home_directory)
    {
        return format!("~{}", path_separator_prefixed(relative_path));
    }
    path.display().to_string()
}

fn path_separator_prefixed(path: &Path) -> String {
    let text = path.display().to_string();
    if text.is_empty() {
        String::new()
    } else {
        format!("/{text}")
    }
}

fn reporting_period_kind_json(kind: &ReportingPeriodKind) -> &'static str {
    match kind {
        ReportingPeriodKind::Daily => "daily",
        ReportingPeriodKind::Weekly => "weekly",
        ReportingPeriodKind::Monthly => "monthly",
        ReportingPeriodKind::AllTime => "all_time",
    }
}

fn period_cost_state_json(period_cost_state: &PeriodCostState) -> JsonPeriodCostState {
    match period_cost_state {
        PeriodCostState::MissingUsageSource => JsonPeriodCostState::MissingUsageSource,
        PeriodCostState::ZeroUsage => JsonPeriodCostState::ZeroUsage,
        PeriodCostState::Complete => JsonPeriodCostState::Complete,
        PeriodCostState::Partial { reasons } => JsonPeriodCostState::Partial {
            reasons: reasons.iter().map(incomplete_cost_reason_json).collect(),
        },
        PeriodCostState::Incomplete { reasons } => JsonPeriodCostState::Incomplete {
            reasons: reasons.iter().map(incomplete_cost_reason_json).collect(),
        },
    }
}

fn cost_state_json(cost_state: &CostState) -> JsonCostState {
    match cost_state {
        CostState::Complete {
            united_states_dollar_cost,
        } => JsonCostState::Complete {
            united_states_dollar_cost: united_states_dollar_cost.to_string(),
        },
        CostState::Partial {
            known_united_states_dollar_cost,
            reasons,
        } => JsonCostState::Partial {
            known_united_states_dollar_cost: known_united_states_dollar_cost.to_string(),
            reasons: reasons.iter().map(incomplete_cost_reason_json).collect(),
        },
        CostState::Incomplete { reasons } => JsonCostState::Incomplete {
            reasons: reasons.iter().map(incomplete_cost_reason_json).collect(),
        },
    }
}

fn incomplete_cost_reason_json(reason: &IncompleteCostReason) -> JsonIncompleteCostReason {
    match reason {
        IncompleteCostReason::UnpricedUsage { model } => JsonIncompleteCostReason::UnpricedUsage {
            model: model.clone(),
        },
        IncompleteCostReason::MissingTokenCategory { token_category } => {
            JsonIncompleteCostReason::MissingTokenCategory {
                token_category: token_category.clone(),
            }
        }
    }
}

fn data_quality_notice_kind_json(kind: &DataQualityNoticeKind) -> &'static str {
    match kind {
        DataQualityNoticeKind::ParseProblem => "parse_problem",
        DataQualityNoticeKind::UnknownSourceFormat => "unknown_source_format",
        DataQualityNoticeKind::MissingTokenSnapshot => "missing_token_snapshot",
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;

    use super::*;
    use crate::cost::{CostState, PricedCodexSession};
    use crate::reporting_period::{PeriodCostState, SummaryTotals};
    use crate::session::{AiCodingAgent, CodexSession, DataQualityNotice, TokenTotals};

    #[test]
    fn json_output_includes_schema_paths_data_quality_and_summary_parity() {
        let source_path = env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/home/person"))
            .join(".codex/sessions/session.jsonl");
        let priced_session = PricedCodexSession {
            codex_session: CodexSession {
                ai_coding_agent: AiCodingAgent::Codex,
                source_path: source_path.clone(),
                session_name: Some("JSON Session Name".to_owned()),
                session_start_time: Utc.with_ymd_and_hms(2026, 5, 10, 10, 0, 0).unwrap(),
                session_last_modified_time: None,
                model: "gpt-5.5".to_owned(),
                reasoning_effort: None,
                project_path: Some(source_path.parent().unwrap().to_path_buf()),
                project_name: Some("sessions".to_owned()),
                is_active: false,
                token_totals: TokenTotals {
                    input_tokens: Some(10),
                    output_tokens: Some(5),
                    total_tokens: Some(15),
                    ..TokenTotals::default()
                },
            },
            cost_state: CostState::Complete {
                united_states_dollar_cost: Decimal::from(2),
            },
            price_schedule_match: Some(crate::cost::PriceScheduleMatch {
                model: "gpt-5.5".to_owned(),
                effective_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                input_tokens_per_million: Decimal::from_str_exact("5.000").unwrap(),
                cache_read_tokens_per_million: Decimal::from_str_exact("0.500").unwrap(),
                cache_write_tokens_per_million: Decimal::from_str_exact("5.000").unwrap(),
                output_tokens_per_million: Decimal::from_str_exact("30.000").unwrap(),
                reasoning_output_tokens_per_million: None,
            }),
        };
        let derived_summary = DerivedSummary {
            current_source_state: CurrentSourceState::Readable {
                path: source_path.parent().unwrap().to_path_buf(),
                is_custom: false,
            },
            headline_periods: vec![crate::reporting_period::HeadlinePeriod {
                kind: ReportingPeriodKind::AllTime,
                local_start_date: None,
                local_end_date: None,
                summary_totals: SummaryTotals {
                    known_united_states_dollar_cost: Decimal::from(2),
                    period_cost_state: PeriodCostState::Complete,
                    token_totals: priced_session.codex_session.token_totals.clone(),
                    session_count: 1,
                },
                previous_period_known_united_states_dollar_cost: None,
                session_detail: vec![priced_session],
            }],
            all_time_detail: Vec::new(),
            data_quality_notices: vec![DataQualityNotice {
                source_path: source_path.clone(),
                line_number: Some(1),
                kind: DataQualityNoticeKind::ParseProblem,
                detail: "invalid JSONL record".to_owned(),
            }],
        };

        let normal_output = build_json_output(
            &derived_summary,
            JsonOutputOptions {
                redact_paths: false,
            },
        )
        .unwrap();
        let redacted_output =
            build_json_output(&derived_summary, JsonOutputOptions { redact_paths: true }).unwrap();

        assert_eq!(normal_output.output_schema_version, OUTPUT_SCHEMA_VERSION);
        assert_eq!(normal_output.output_schema_version, 3);
        assert_eq!(
            normal_output.usage_source.path,
            source_path.parent().unwrap().display().to_string()
        );
        assert_eq!(normal_output.headline_periods[0].session_count, 1);
        assert_eq!(
            normal_output.headline_periods[0].session_detail[0].ai_coding_agent,
            "Codex"
        );
        assert_eq!(
            normal_output.headline_periods[0].session_detail[0]
                .session_name
                .as_deref(),
            Some("JSON Session Name")
        );
        assert_eq!(
            normal_output.headline_periods[0].known_united_states_dollar_cost,
            "2"
        );
        assert_eq!(
            normal_output.headline_periods[0].previous_period_known_united_states_dollar_cost,
            None
        );
        assert_eq!(
            normal_output.headline_periods[0]
                .known_united_states_dollar_cost_change_from_previous_period,
            None
        );
        assert_eq!(normal_output.data_quality_detail[0].kind, "parse_problem");
        assert!(redacted_output.usage_source.path.starts_with('~'));
    }
}
