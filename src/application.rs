//! Command-line application orchestration.

use std::error::Error;
use std::io::{self, IsTerminal};
use std::path::PathBuf;

use clap::Parser;

use crate::cost::price_sessions;
use crate::output::{JsonOutputOptions, build_json_output};
use crate::price_catalog::PriceCatalog;
use crate::reporting_period::{DerivedSummary, build_derived_summary};
use crate::session::read_codex_sessions;
use crate::terminal_interface::{render_terminal_summary, run_terminal_interface};
use crate::usage_source::{UsageSourceInventory, build_usage_source_inventory};

/// Command-line options for one AI Usage run.
#[derive(Debug, Parser)]
#[command(
    name = "ai-usage",
    about = "Show local AI coding agent usage cost summaries"
)]
pub struct RunOptions {
    /// Custom Usage Source path for this run.
    #[arg(long = "usage-source")]
    pub custom_usage_source: Option<PathBuf>,

    /// Emit JSON Output instead of the interactive terminal interface.
    #[arg(long = "json")]
    pub emit_json_output: bool,

    /// Redact home-directory-sensitive prefixes in JSON Output.
    #[arg(long = "redact-paths")]
    pub redact_paths: bool,
}

/// Runs the application from process arguments.
pub fn run() -> Result<(), Box<dyn Error>> {
    let run_options = RunOptions::parse();
    let custom_usage_source = run_options.custom_usage_source.clone();
    let derived_summary = derive_summary_from_custom_usage_source(custom_usage_source.clone())?;

    if run_options.emit_json_output {
        let json_output = build_json_output(
            &derived_summary,
            JsonOutputOptions {
                redact_paths: run_options.redact_paths,
            },
        )?;
        println!("{}", serde_json::to_string_pretty(&json_output)?);
        return Ok(());
    }

    if io::stdout().is_terminal() {
        run_terminal_interface(derived_summary, move || {
            derive_summary_from_custom_usage_source(custom_usage_source.clone())
        })?;
    } else {
        println!("{}", render_terminal_summary(&derived_summary));
    }

    Ok(())
}

/// Builds the current Derived Summary from Run Options.
pub fn derive_summary_from_custom_usage_source(
    custom_usage_source: Option<PathBuf>,
) -> Result<DerivedSummary, Box<dyn Error>> {
    let usage_source_inventory = build_usage_source_inventory(custom_usage_source)?;
    derive_summary_from_inventory(&usage_source_inventory)
}

/// Builds the current Derived Summary from a Usage Source Inventory.
pub fn derive_summary_from_inventory(
    usage_source_inventory: &UsageSourceInventory,
) -> Result<DerivedSummary, Box<dyn Error>> {
    let price_catalog = PriceCatalog::bundled();
    let parsed_usage = read_codex_sessions(usage_source_inventory)?;
    let priced_sessions = price_sessions(parsed_usage.codex_sessions, &price_catalog);

    Ok(build_derived_summary(
        usage_source_inventory.current_source_state.clone(),
        priced_sessions,
        parsed_usage.data_quality_notices,
    ))
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;

    use crate::reporting_period::ReportingPeriodKind;

    use super::*;

    #[test]
    fn deriving_summary_again_reflects_current_source_state_changes() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let first_session_path = temporary_directory.path().join("first.jsonl");
        let second_session_path = temporary_directory.path().join("second.jsonl");
        write_session_file(&first_session_path, "first", 10);

        let first_summary =
            derive_summary_from_custom_usage_source(Some(temporary_directory.path().to_path_buf()))
                .expect("first summary");

        assert_eq!(all_time_session_count(&first_summary), 1);

        std::fs::remove_file(&first_session_path).expect("deleted session file");
        write_session_file(&second_session_path, "second", 20);

        let reloaded_summary =
            derive_summary_from_custom_usage_source(Some(temporary_directory.path().to_path_buf()))
                .expect("reloaded summary");

        assert_eq!(all_time_session_count(&reloaded_summary), 1);
        assert_eq!(
            reloaded_summary
                .headline_periods
                .iter()
                .find(|period| period.kind == ReportingPeriodKind::AllTime)
                .and_then(|period| period.session_detail.first())
                .and_then(|priced_session| priced_session.codex_session.project_name.as_deref()),
            Some("second")
        );
    }

    fn all_time_session_count(derived_summary: &DerivedSummary) -> usize {
        derived_summary
            .headline_periods
            .iter()
            .find(|period| period.kind == ReportingPeriodKind::AllTime)
            .map(|period| period.summary_totals.session_count)
            .unwrap_or(0)
    }

    fn write_session_file(path: &Path, project_name: &str, output_tokens: u64) {
        let mut file = File::create(path).expect("session file");
        writeln!(
            file,
            r#"{{"timestamp":"2026-05-10T10:00:00Z","type":"session_meta","payload":{{"timestamp":"2026-05-10T10:00:00Z","cwd":"/tmp/{project_name}"}}}}"#
        )
        .expect("session metadata");
        writeln!(
            file,
            r#"{{"timestamp":"2026-05-10T10:00:01Z","type":"turn_context","payload":{{"model":"gpt-5.5"}}}}"#
        )
        .expect("turn context");
        writeln!(
            file,
            r#"{{"timestamp":"2026-05-10T10:00:02Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":10,"output_tokens":{output_tokens},"total_tokens":{}}}}}}}}}"#,
            10 + output_tokens
        )
        .expect("token snapshot");
    }
}
