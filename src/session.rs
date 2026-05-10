//! Structured parsing for Codex Session Files.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use walkdir::WalkDir;

use crate::usage_source::UsageSourceResolution;

/// Parsed usage and Data Quality Notices from a Usage Source.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParsedUsage {
    /// Codex Sessions built from readable Codex Session Files.
    pub codex_sessions: Vec<CodexSession>,
    /// Notices describing records excluded from Derived Summaries.
    pub data_quality_notices: Vec<DataQualityNotice>,
}

/// A single Codex work session derived from one Codex Session File.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexSession {
    /// Source file path.
    pub source_path: PathBuf,
    /// Session Start Time.
    pub session_start_time: DateTime<Utc>,
    /// Exact recorded Model name.
    pub model: String,
    /// Full project path when available.
    pub project_path: Option<PathBuf>,
    /// Compact Project Name when available.
    pub project_name: Option<String>,
    /// Whether no completion record was observed.
    pub is_active: bool,
    /// Session-level token totals from the selected Token Snapshot.
    pub token_totals: TokenTotals,
}

/// Token categories preserved from a Token Snapshot.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct TokenTotals {
    /// Input Tokens.
    pub input_tokens: Option<u64>,
    /// Non-Cached Input Tokens.
    pub non_cached_input_tokens: Option<u64>,
    /// Cache Read Tokens.
    pub cache_read_tokens: Option<u64>,
    /// Cache Write Tokens.
    pub cache_write_tokens: Option<u64>,
    /// Output Tokens.
    pub output_tokens: Option<u64>,
    /// Reasoning Output Tokens.
    pub reasoning_output_tokens: Option<u64>,
    /// Total Tokens.
    pub total_tokens: Option<u64>,
}

/// A parse or source-quality problem that did not fail the whole run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataQualityNotice {
    /// File path associated with the problem.
    pub source_path: PathBuf,
    /// One-based line number when applicable.
    pub line_number: Option<usize>,
    /// Stable notice kind.
    pub kind: DataQualityNoticeKind,
    /// Human-readable detail.
    pub detail: String,
}

/// Kinds of source records excluded or partially excluded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataQualityNoticeKind {
    /// JSONL record could not be decoded.
    ParseProblem,
    /// JSON was valid but the record shape is not recognized for usage.
    UnknownSourceFormat,
    /// No usable Token Snapshot was found.
    MissingTokenSnapshot,
}

#[derive(Debug, Deserialize)]
struct CodexRecord {
    #[serde(default)]
    timestamp: Option<DateTime<Utc>>,
    #[serde(rename = "type")]
    record_type: String,
    #[serde(default)]
    payload: Value,
}

/// Reads Codex Sessions from the resolved Usage Source.
pub fn read_codex_sessions(
    usage_source_resolution: &UsageSourceResolution,
) -> std::io::Result<ParsedUsage> {
    let mut parsed_usage = ParsedUsage::default();

    let UsageSourceResolution::Readable { path, .. } = usage_source_resolution else {
        return Ok(parsed_usage);
    };

    for source_path in codex_session_file_paths(path)? {
        match parse_codex_session_file(&source_path) {
            Ok((Some(codex_session), mut data_quality_notices)) => {
                parsed_usage.codex_sessions.push(codex_session);
                parsed_usage
                    .data_quality_notices
                    .append(&mut data_quality_notices);
            }
            Ok((None, mut data_quality_notices)) => parsed_usage
                .data_quality_notices
                .append(&mut data_quality_notices),
            Err(error) => parsed_usage.data_quality_notices.push(DataQualityNotice {
                source_path,
                line_number: None,
                kind: DataQualityNoticeKind::ParseProblem,
                detail: error.to_string(),
            }),
        }
    }

    Ok(parsed_usage)
}

fn codex_session_file_paths(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    if fs::metadata(path)?.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    let mut file_paths = WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|file_path| {
            file_path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .collect::<Vec<_>>();
    file_paths.sort();
    Ok(file_paths)
}

fn parse_codex_session_file(
    source_path: &Path,
) -> std::io::Result<(Option<CodexSession>, Vec<DataQualityNotice>)> {
    let file = File::open(source_path)?;
    let reader = BufReader::new(file);
    let mut data_quality_notices = Vec::new();
    let mut session_start_time = None;
    let mut model = None;
    let mut project_path = None;
    let mut final_token_snapshot = None;
    let mut saw_completion_record = false;

    for (line_index, line_result) in reader.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line_result?;
        let Ok(record) = serde_json::from_str::<CodexRecord>(&line) else {
            data_quality_notices.push(notice(
                source_path,
                Some(line_number),
                DataQualityNoticeKind::ParseProblem,
                "invalid JSONL record",
            ));
            continue;
        };

        match record.record_type.as_str() {
            "session_meta" => {
                session_start_time = record
                    .payload
                    .get("timestamp")
                    .and_then(parse_time_value)
                    .or(record.timestamp);
                project_path = record
                    .payload
                    .get("cwd")
                    .and_then(Value::as_str)
                    .map(PathBuf::from);
            }
            "turn_context" => {
                if model.is_none() {
                    model = record
                        .payload
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
                if project_path.is_none() {
                    project_path = record
                        .payload
                        .get("cwd")
                        .and_then(Value::as_str)
                        .map(PathBuf::from);
                }
            }
            "event_msg" => {
                if record.payload.get("type").and_then(Value::as_str) == Some("token_count")
                    && let Some(info) = record.payload.get("info")
                    && !info.is_null()
                {
                    final_token_snapshot = token_totals_from_info(info);
                }
                if record.payload.get("type").and_then(Value::as_str) == Some("task_complete") {
                    saw_completion_record = true;
                }
            }
            "response_item" => {}
            _ => data_quality_notices.push(notice(
                source_path,
                Some(line_number),
                DataQualityNoticeKind::UnknownSourceFormat,
                "unknown Codex record type",
            )),
        }
    }

    let Some(token_totals) = final_token_snapshot else {
        data_quality_notices.push(notice(
            source_path,
            None,
            DataQualityNoticeKind::MissingTokenSnapshot,
            "no usable Token Snapshot found",
        ));
        return Ok((None, data_quality_notices));
    };

    let session_start_time = session_start_time.unwrap_or_else(Utc::now);
    let project_name = project_path
        .as_ref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .map(str::to_owned);

    Ok((
        Some(CodexSession {
            source_path: source_path.to_path_buf(),
            session_start_time,
            model: model.unwrap_or_else(|| "unknown".to_owned()),
            project_path,
            project_name,
            is_active: !saw_completion_record,
            token_totals,
        }),
        data_quality_notices,
    ))
}

fn token_totals_from_info(info: &Value) -> Option<TokenTotals> {
    let total_token_usage = info.get("total_token_usage")?;
    let input_tokens = get_u64(total_token_usage, "input_tokens");
    let cache_read_tokens = get_u64(total_token_usage, "cached_input_tokens")
        .or_else(|| get_u64(total_token_usage, "cache_read_tokens"));
    let non_cached_input_tokens = match (input_tokens, cache_read_tokens) {
        (Some(input_tokens), Some(cache_read_tokens)) if input_tokens >= cache_read_tokens => {
            Some(input_tokens - cache_read_tokens)
        }
        _ => get_u64(total_token_usage, "non_cached_input_tokens"),
    };

    Some(TokenTotals {
        input_tokens,
        non_cached_input_tokens,
        cache_read_tokens,
        cache_write_tokens: get_u64(total_token_usage, "cache_creation_input_tokens")
            .or_else(|| get_u64(total_token_usage, "cache_write_tokens")),
        output_tokens: get_u64(total_token_usage, "output_tokens"),
        reasoning_output_tokens: get_u64(total_token_usage, "reasoning_output_tokens"),
        total_tokens: get_u64(total_token_usage, "total_tokens").or_else(|| {
            match (input_tokens, get_u64(total_token_usage, "output_tokens")) {
                (Some(input_tokens), Some(output_tokens)) => Some(input_tokens + output_tokens),
                _ => None,
            }
        }),
    })
}

fn get_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn parse_time_value(value: &Value) -> Option<DateTime<Utc>> {
    value.as_str()?.parse::<DateTime<Utc>>().ok()
}

fn notice(
    source_path: &Path,
    line_number: Option<usize>,
    kind: DataQualityNoticeKind,
    detail: &str,
) -> DataQualityNotice {
    DataQualityNotice {
        source_path: source_path.to_path_buf(),
        line_number,
        kind,
        detail: detail.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn parses_valid_session_file_without_prompt_content() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let session_file_path = temporary_directory.path().join("session.jsonl");
        write_session_file(
            &session_file_path,
            &[
                r#"{"timestamp":"2026-05-10T10:00:00Z","type":"session_meta","payload":{"timestamp":"2026-05-10T09:59:00Z","cwd":"/home/person/project"}}"#,
                r#"{"timestamp":"2026-05-10T10:00:01Z","type":"turn_context","payload":{"model":"gpt-5.5","cwd":"/home/person/project"}}"#,
                r#"{"timestamp":"2026-05-10T10:00:02Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"secret prompt"}]}}"#,
                r#"{"timestamp":"2026-05-10T10:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":250,"cache_creation_input_tokens":100,"output_tokens":300,"reasoning_output_tokens":20,"total_tokens":1300}}}}"#,
                r#"{"timestamp":"2026-05-10T10:00:04Z","type":"event_msg","payload":{"type":"task_complete"}}"#,
            ],
        );

        let parsed_usage = read_codex_sessions(&UsageSourceResolution::Readable {
            path: temporary_directory.path().to_path_buf(),
            is_custom: true,
        })
        .expect("parsed usage");

        assert_eq!(parsed_usage.codex_sessions.len(), 1);
        let codex_session = &parsed_usage.codex_sessions[0];
        assert_eq!(codex_session.model, "gpt-5.5");
        assert_eq!(codex_session.project_name.as_deref(), Some("project"));
        assert!(!codex_session.is_active);
        assert_eq!(
            codex_session.token_totals.non_cached_input_tokens,
            Some(750)
        );
        assert_eq!(codex_session.token_totals.cache_read_tokens, Some(250));
        assert_eq!(codex_session.token_totals.total_tokens, Some(1300));
    }

    #[test]
    fn final_token_snapshot_is_selected_instead_of_summing_cumulative_snapshots() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let session_file_path = temporary_directory.path().join("session.jsonl");
        write_session_file(
            &session_file_path,
            &[
                r#"{"timestamp":"2026-05-10T10:00:00Z","type":"session_meta","payload":{"timestamp":"2026-05-10T10:00:00Z"}}"#,
                r#"{"timestamp":"2026-05-10T10:00:01Z","type":"turn_context","payload":{"model":"gpt-5.5"}}"#,
                r#"{"timestamp":"2026-05-10T10:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"output_tokens":20,"total_tokens":120}}}}"#,
                r#"{"timestamp":"2026-05-10T10:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":150,"output_tokens":40,"total_tokens":190}}}}"#,
            ],
        );

        let parsed_usage = read_codex_sessions(&UsageSourceResolution::Readable {
            path: session_file_path,
            is_custom: true,
        })
        .expect("parsed usage");

        assert_eq!(
            parsed_usage.codex_sessions[0].token_totals.input_tokens,
            Some(150)
        );
        assert_eq!(
            parsed_usage.codex_sessions[0].token_totals.output_tokens,
            Some(40)
        );
        assert_eq!(
            parsed_usage.codex_sessions[0].token_totals.total_tokens,
            Some(190)
        );
        assert!(parsed_usage.codex_sessions[0].is_active);
    }

    #[test]
    fn parse_problem_and_missing_snapshot_create_notices_without_failing_run() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let session_file_path = temporary_directory.path().join("session.jsonl");
        write_session_file(
            &session_file_path,
            &[
                "not-json",
                r#"{"timestamp":"2026-05-10T10:00:00Z","type":"mystery","payload":{}}"#,
            ],
        );

        let parsed_usage = read_codex_sessions(&UsageSourceResolution::Readable {
            path: session_file_path,
            is_custom: true,
        })
        .expect("parsed usage");

        assert!(parsed_usage.codex_sessions.is_empty());
        assert_eq!(parsed_usage.data_quality_notices.len(), 3);
        assert_eq!(
            parsed_usage.data_quality_notices[0].kind,
            DataQualityNoticeKind::ParseProblem
        );
        assert_eq!(
            parsed_usage.data_quality_notices[1].kind,
            DataQualityNoticeKind::UnknownSourceFormat
        );
        assert_eq!(
            parsed_usage.data_quality_notices[2].kind,
            DataQualityNoticeKind::MissingTokenSnapshot
        );
    }

    fn write_session_file(path: &Path, lines: &[&str]) {
        let mut file = File::create(path).expect("session file");
        for line in lines {
            writeln!(file, "{line}").expect("session line");
        }
    }
}
