//! Structured parsing for local AI Coding Agent session files.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use walkdir::WalkDir;

use crate::usage_source::UsageSourceResolution;

/// Parsed usage and Data Quality Notices from a Usage Source.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParsedUsage {
    /// AI Coding Agent Sessions built from readable session files.
    pub codex_sessions: Vec<CodexSession>,
    /// Notices describing records excluded from Derived Summaries.
    pub data_quality_notices: Vec<DataQualityNotice>,
}

/// AI Coding Agent that produced a session file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiCodingAgent {
    /// OpenAI Codex CLI.
    Codex,
    /// Pi Coding Agent.
    Pi,
    /// OpenCode.
    Opencode,
}

impl AiCodingAgent {
    /// Display label for Session Detail columns.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Pi => "Pi",
            Self::Opencode => "OpenCode",
        }
    }
}

impl CodexSession {
    /// Compact Model label for Session Detail columns.
    pub fn compact_model_label(&self) -> String {
        match self.reasoning_effort.as_deref() {
            Some(reasoning_effort) if !reasoning_effort.is_empty() => {
                format!("{} · {}", self.model, reasoning_effort)
            }
            _ => self.model.clone(),
        }
    }

    /// Time shown in the Session Detail Time column.
    pub fn session_detail_time(&self) -> DateTime<Utc> {
        self.session_last_modified_time
            .unwrap_or(self.session_start_time)
    }
}

/// A single work session derived from one AI Coding Agent session file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexSession {
    /// AI Coding Agent that produced the session.
    pub ai_coding_agent: AiCodingAgent,
    /// Source file path.
    pub source_path: PathBuf,
    /// Source-recorded Session Name when available.
    pub session_name: Option<String>,
    /// Session Start Time.
    pub session_start_time: DateTime<Utc>,
    /// Source last-modified time when available.
    pub session_last_modified_time: Option<DateTime<Utc>>,
    /// Exact recorded Model name.
    pub model: String,
    /// Source-recorded reasoning effort or thinking level when available.
    pub reasoning_effort: Option<String>,
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

/// Reads AI Coding Agent Sessions from the resolved Usage Source.
pub fn read_codex_sessions(
    usage_source_resolution: &UsageSourceResolution,
) -> std::io::Result<ParsedUsage> {
    let mut parsed_usage = ParsedUsage::default();

    let UsageSourceResolution::Readable { path, is_custom } = usage_source_resolution else {
        return Ok(parsed_usage);
    };

    let session_file_paths = if *is_custom {
        session_file_paths(path)?
    } else {
        default_session_file_paths(path)?
    };

    for source_path in session_file_paths {
        match parse_session_file(&source_path) {
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

    if !*is_custom || path.is_dir() {
        let opencode_database_path = if *is_custom {
            opencode_database_path(path)
        } else {
            opencode_database_path(&crate::usage_source::default_opencode_sessions_directory())
        };
        if fs::metadata(&opencode_database_path).is_ok() {
            match parse_opencode_database(&opencode_database_path) {
                Ok(mut opencode_sessions) => {
                    parsed_usage.codex_sessions.append(&mut opencode_sessions);
                }
                Err(error) => parsed_usage.data_quality_notices.push(DataQualityNotice {
                    source_path: opencode_database_path,
                    line_number: None,
                    kind: DataQualityNoticeKind::ParseProblem,
                    detail: error.to_string(),
                }),
            }
        }
    }

    Ok(parsed_usage)
}

fn opencode_database_path(path: &Path) -> PathBuf {
    if path
        .file_name()
        .is_some_and(|file_name| file_name == "opencode.db")
    {
        return path.to_path_buf();
    }
    path.join("opencode.db")
}

fn default_session_file_paths(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut file_paths = session_file_paths(path)?;
    let pi_sessions_directory = crate::usage_source::default_pi_sessions_directory();
    if pi_sessions_directory != path && fs::metadata(&pi_sessions_directory).is_ok() {
        file_paths.extend(session_file_paths(&pi_sessions_directory)?);
    }
    let opencode_sessions_directory = crate::usage_source::default_opencode_sessions_directory();
    if opencode_sessions_directory != path && fs::metadata(&opencode_sessions_directory).is_ok() {
        file_paths.extend(session_file_paths(&opencode_sessions_directory)?);
    }
    file_paths.sort();
    Ok(file_paths)
}

fn session_file_paths(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    if fs::metadata(path)?.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    let mut file_paths = WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|file_path| is_usage_file_path(file_path))
        .collect::<Vec<_>>();
    file_paths.sort();
    Ok(file_paths)
}

fn is_usage_file_path(file_path: &Path) -> bool {
    match file_path
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some("jsonl") => true,
        Some("json") => file_path.components().any(|component| {
            component.as_os_str() == "session" || component.as_os_str() == "message"
        }),
        _ => false,
    }
}

fn parse_session_file(
    source_path: &Path,
) -> std::io::Result<(Option<CodexSession>, Vec<DataQualityNotice>)> {
    if let Some(parsed_pi_session) = parse_pi_session_file(source_path)? {
        return Ok(parsed_pi_session);
    }
    if let Some(parsed_opencode_session) = parse_opencode_session_file(source_path)? {
        return Ok(parsed_opencode_session);
    }
    parse_codex_session_file(source_path)
}

fn parse_codex_session_file(
    source_path: &Path,
) -> std::io::Result<(Option<CodexSession>, Vec<DataQualityNotice>)> {
    let file = File::open(source_path)?;
    let source_modified_time = source_modified_time(source_path);
    let reader = BufReader::new(file);
    let mut data_quality_notices = Vec::new();
    let mut session_start_time = None;
    let mut session_name = None;
    let mut model = None;
    let mut reasoning_effort = None;
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
                session_name = source_recorded_session_name(&record.payload).or(session_name);
            }
            "turn_context" => {
                if model.is_none() {
                    model = record
                        .payload
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
                if reasoning_effort.is_none() {
                    reasoning_effort = record
                        .payload
                        .get("effort")
                        .or_else(|| record.payload.get("reasoning_effort"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| {
                            record
                                .payload
                                .get("collaboration_mode")
                                .and_then(|collaboration_mode| collaboration_mode.get("settings"))
                                .and_then(|settings| settings.get("reasoning_effort"))
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                        });
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
    let project_name = project_name_from_path(project_path.as_deref());

    Ok((
        Some(CodexSession {
            ai_coding_agent: AiCodingAgent::Codex,
            source_path: source_path.to_path_buf(),
            session_name,
            session_start_time,
            session_last_modified_time: source_modified_time,
            model: model.unwrap_or_else(|| "unknown".to_owned()),
            reasoning_effort,
            project_path,
            project_name,
            is_active: !saw_completion_record,
            token_totals,
        }),
        data_quality_notices,
    ))
}

fn parse_pi_session_file(
    source_path: &Path,
) -> std::io::Result<Option<(Option<CodexSession>, Vec<DataQualityNotice>)>> {
    let file = File::open(source_path)?;
    let source_modified_time = source_modified_time(source_path);
    let reader = BufReader::new(file);
    let mut data_quality_notices = Vec::new();
    let mut is_pi_session = false;
    let mut session_start_time = None;
    let mut session_name = None;
    let mut model = None;
    let mut reasoning_effort = None;
    let mut project_path = None;
    let mut token_totals = TokenTotals::default();
    let mut saw_assistant_message = false;

    for (line_index, line_result) in reader.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line_result?;
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            data_quality_notices.push(notice(
                source_path,
                Some(line_number),
                DataQualityNoticeKind::ParseProblem,
                "invalid JSONL record",
            ));
            continue;
        };

        match record.get("type").and_then(Value::as_str) {
            Some("session") => {
                is_pi_session = true;
                session_start_time = record.get("timestamp").and_then(parse_time_value);
                session_name = source_recorded_session_name(&record).or(session_name);
                project_path = record.get("cwd").and_then(Value::as_str).map(PathBuf::from);
            }
            Some("message") if is_pi_session => {
                let Some(message) = record.get("message") else {
                    continue;
                };
                if message.get("role").and_then(Value::as_str) == Some("assistant") {
                    saw_assistant_message = true;
                    model = message
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or(model);
                    reasoning_effort = message
                        .get("reasoningEffort")
                        .or_else(|| message.get("reasoning_effort"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or(reasoning_effort);
                    if let Some(usage) = message.get("usage") {
                        add_pi_usage(&mut token_totals, usage);
                    }
                }
            }
            Some("model_change") if is_pi_session => {
                model = record
                    .get("modelId")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or(model);
            }
            Some("thinking_level_change") if is_pi_session => {
                reasoning_effort = record
                    .get("thinkingLevel")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or(reasoning_effort);
            }
            Some(_) if is_pi_session => {}
            _ => {}
        }
    }

    if !is_pi_session {
        return Ok(None);
    }

    if !saw_assistant_message || token_totals.total_tokens.is_none() {
        data_quality_notices.push(notice(
            source_path,
            None,
            DataQualityNoticeKind::MissingTokenSnapshot,
            "no usable Token Snapshot found",
        ));
        return Ok(Some((None, data_quality_notices)));
    }

    let session_start_time = session_start_time.unwrap_or_else(Utc::now);
    let project_name = project_name_from_path(project_path.as_deref());

    Ok(Some((
        Some(CodexSession {
            ai_coding_agent: AiCodingAgent::Pi,
            source_path: source_path.to_path_buf(),
            session_name,
            session_start_time,
            session_last_modified_time: source_modified_time,
            model: model.unwrap_or_else(|| "unknown".to_owned()),
            reasoning_effort,
            project_path,
            project_name,
            is_active: false,
            token_totals,
        }),
        data_quality_notices,
    )))
}

fn add_pi_usage(token_totals: &mut TokenTotals, usage: &Value) {
    add_optional_u64(&mut token_totals.input_tokens, get_u64(usage, "input"));
    add_optional_u64(&mut token_totals.output_tokens, get_u64(usage, "output"));
    add_optional_u64(
        &mut token_totals.cache_read_tokens,
        get_u64(usage, "cacheRead"),
    );
    add_optional_u64(
        &mut token_totals.cache_write_tokens,
        get_u64(usage, "cacheWrite"),
    );
    add_optional_u64(
        &mut token_totals.total_tokens,
        get_u64(usage, "totalTokens").or_else(|| {
            match (get_u64(usage, "input"), get_u64(usage, "output")) {
                (Some(input_tokens), Some(output_tokens)) => Some(input_tokens + output_tokens),
                _ => None,
            }
        }),
    );

    token_totals.non_cached_input_tokens =
        match (token_totals.input_tokens, token_totals.cache_read_tokens) {
            (Some(input_tokens), Some(cache_read_tokens)) if input_tokens >= cache_read_tokens => {
                Some(input_tokens - cache_read_tokens)
            }
            _ => token_totals.non_cached_input_tokens,
        };
}

fn parse_opencode_session_file(
    source_path: &Path,
) -> std::io::Result<Option<(Option<CodexSession>, Vec<DataQualityNotice>)>> {
    let source_modified_time = source_modified_time(source_path);
    if source_path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        let file_content = fs::read_to_string(source_path)?;
        if let Ok(record) = serde_json::from_str::<Value>(&file_content) {
            return Ok(parse_opencode_records(
                source_path,
                &[record],
                source_modified_time,
            ));
        }
    }

    let file = File::open(source_path)?;
    let reader = BufReader::new(file);
    let mut data_quality_notices = Vec::new();
    let mut records = Vec::new();

    for (line_index, line_result) in reader.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line_result?;
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            data_quality_notices.push(notice(
                source_path,
                Some(line_number),
                DataQualityNoticeKind::ParseProblem,
                "invalid JSON record",
            ));
            continue;
        };
        records.push(record);
    }

    Ok(
        parse_opencode_records(source_path, &records, source_modified_time).map(
            |(codex_session, mut notices)| {
                data_quality_notices.append(&mut notices);
                (codex_session, data_quality_notices)
            },
        ),
    )
}

fn parse_opencode_records(
    source_path: &Path,
    records: &[Value],
    source_modified_time: Option<DateTime<Utc>>,
) -> Option<(Option<CodexSession>, Vec<DataQualityNotice>)> {
    let mut data_quality_notices = Vec::new();
    let mut is_opencode_session = false;
    let mut session_start_time = None;
    let mut session_name = None;
    let mut session_last_modified_time = source_modified_time;
    let mut model = None;
    let mut reasoning_effort = None;
    let mut project_path = None;
    let mut token_totals = TokenTotals::default();
    let mut saw_assistant_message = false;

    for record in records {
        if is_opencode_metadata_record(&record) {
            is_opencode_session = true;
            session_start_time = opencode_created_time(&record).or(session_start_time);
            session_name = source_recorded_session_name(record).or(session_name);
            session_last_modified_time =
                opencode_last_modified_time(&record).or(session_last_modified_time);
            project_path = opencode_project_path(&record).or(project_path);
        }

        let message = record.get("message").unwrap_or(&record);
        if opencode_message_role(message) == Some("assistant") {
            is_opencode_session = true;
            saw_assistant_message = true;
            model = opencode_model(message).or(model);
            reasoning_effort = opencode_reasoning_effort(message).or(reasoning_effort);
            if let Some(usage) = message.get("usage") {
                add_opencode_usage(&mut token_totals, usage);
            }
        }
    }

    if !is_opencode_session {
        return None;
    }

    if !saw_assistant_message || token_totals.total_tokens.is_none() {
        data_quality_notices.push(notice(
            source_path,
            None,
            DataQualityNoticeKind::MissingTokenSnapshot,
            "no usable Token Snapshot found",
        ));
        return Some((None, data_quality_notices));
    }

    let session_start_time = session_start_time.unwrap_or_else(Utc::now);
    let project_name = project_name_from_path(project_path.as_deref());

    Some((
        Some(CodexSession {
            ai_coding_agent: AiCodingAgent::Opencode,
            source_path: source_path.to_path_buf(),
            session_name,
            session_start_time,
            session_last_modified_time,
            model: model.unwrap_or_else(|| "unknown".to_owned()),
            reasoning_effort,
            project_path,
            project_name,
            is_active: false,
            token_totals,
        }),
        data_quality_notices,
    ))
}

fn is_opencode_metadata_record(record: &Value) -> bool {
    record
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|identifier| identifier.starts_with("ses_"))
        || record.get("sessionID").is_some()
        || record.get("session_id").is_some()
}

fn opencode_created_time(record: &Value) -> Option<DateTime<Utc>> {
    record
        .get("time")
        .and_then(|time| time.get("created"))
        .and_then(parse_time_value)
        .or_else(|| record.get("created").and_then(parse_time_value))
        .or_else(|| record.get("timestamp").and_then(parse_time_value))
}

fn opencode_last_modified_time(record: &Value) -> Option<DateTime<Utc>> {
    record
        .get("time")
        .and_then(|time| time.get("updated").or_else(|| time.get("modified")))
        .and_then(parse_time_value)
        .or_else(|| record.get("updated").and_then(parse_time_value))
        .or_else(|| record.get("modified").and_then(parse_time_value))
}

fn opencode_project_path(record: &Value) -> Option<PathBuf> {
    record
        .get("cwd")
        .or_else(|| record.get("directory"))
        .or_else(|| record.get("path"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
}

fn opencode_message_role(message: &Value) -> Option<&str> {
    message
        .get("role")
        .or_else(|| message.get("author").and_then(|author| author.get("role")))
        .and_then(Value::as_str)
}

fn opencode_model(message: &Value) -> Option<String> {
    message
        .get("model")
        .and_then(opencode_model_identifier)
        .or_else(|| {
            message
                .get("modelID")
                .or_else(|| message.get("model_id"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
}

/// Extracts the recorded OpenCode model identifier from string or object model values.
fn opencode_model_identifier(model: &Value) -> Option<String> {
    model.as_str().map(str::to_owned).or_else(|| {
        model
            .get("id")
            .or_else(|| model.get("modelID"))
            .or_else(|| model.get("model_id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

/// Extracts source-recorded OpenCode reasoning effort from model configuration values.
fn opencode_model_reasoning_effort(model: &Value) -> Option<String> {
    model
        .get("variant")
        .or_else(|| model.get("reasoningEffort"))
        .or_else(|| model.get("reasoning_effort"))
        .or_else(|| model.get("thinkingLevel"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn opencode_reasoning_effort(message: &Value) -> Option<String> {
    message
        .get("reasoningEffort")
        .or_else(|| message.get("reasoning_effort"))
        .or_else(|| message.get("thinkingLevel"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            message
                .get("model")
                .and_then(opencode_model_reasoning_effort)
        })
}

fn add_opencode_usage(token_totals: &mut TokenTotals, usage: &Value) {
    add_optional_u64(
        &mut token_totals.input_tokens,
        get_u64(usage, "input").or_else(|| get_u64(usage, "inputTokens")),
    );
    add_optional_u64(
        &mut token_totals.output_tokens,
        get_u64(usage, "output").or_else(|| get_u64(usage, "outputTokens")),
    );
    add_optional_u64(
        &mut token_totals.cache_read_tokens,
        get_u64(usage, "cacheRead").or_else(|| get_u64(usage, "cacheReadTokens")),
    );
    add_optional_u64(
        &mut token_totals.cache_write_tokens,
        get_u64(usage, "cacheWrite").or_else(|| get_u64(usage, "cacheWriteTokens")),
    );
    add_optional_u64(
        &mut token_totals.reasoning_output_tokens,
        get_u64(usage, "reasoning").or_else(|| get_u64(usage, "reasoningTokens")),
    );
    add_optional_u64(
        &mut token_totals.total_tokens,
        get_u64(usage, "totalTokens").or_else(|| {
            match (get_u64(usage, "input"), get_u64(usage, "output")) {
                (Some(input_tokens), Some(output_tokens)) => Some(input_tokens + output_tokens),
                _ => None,
            }
        }),
    );

    token_totals.non_cached_input_tokens =
        match (token_totals.input_tokens, token_totals.cache_read_tokens) {
            (Some(input_tokens), Some(cache_read_tokens)) if input_tokens >= cache_read_tokens => {
                Some(input_tokens - cache_read_tokens)
            }
            _ => token_totals.non_cached_input_tokens,
        };
}

fn parse_opencode_database(source_path: &Path) -> rusqlite::Result<Vec<CodexSession>> {
    let connection =
        Connection::open_with_flags(source_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut statement = connection.prepare(
        "select s.id, s.time_created, s.time_updated, s.directory, s.model, s.title, m.data
         from session s
         join message m on m.session_id = s.id
         order by s.time_created, m.time_created, m.id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;

    let mut sessions = Vec::<CodexSession>::new();
    let mut current_session_identifier = None::<String>;
    let mut current_session = None::<CodexSession>;

    for row in rows {
        let (
            session_identifier,
            session_time_created,
            session_time_updated,
            directory,
            session_model,
            session_name,
            message_data,
        ) = row?;
        if current_session_identifier.as_deref() != Some(session_identifier.as_str()) {
            if let Some(session) = current_session.take()
                && session.token_totals.total_tokens.is_some()
            {
                sessions.push(session);
            }

            current_session_identifier = Some(session_identifier.clone());
            let parsed_session_model = session_model
                .as_deref()
                .and_then(|model| serde_json::from_str::<Value>(model).ok());
            let model = parsed_session_model
                .as_ref()
                .and_then(opencode_model_identifier)
                .or_else(|| session_model.clone())
                .unwrap_or_else(|| "unknown".to_owned());
            let reasoning_effort = parsed_session_model
                .as_ref()
                .and_then(opencode_model_reasoning_effort);
            let project_path = Some(PathBuf::from(directory));
            let project_name = project_name_from_path(project_path.as_deref());
            current_session = Some(CodexSession {
                ai_coding_agent: AiCodingAgent::Opencode,
                source_path: source_path.to_path_buf(),
                session_name: non_empty_string(session_name.as_str()),
                session_start_time: DateTime::from_timestamp_millis(session_time_created)
                    .unwrap_or_else(Utc::now),
                session_last_modified_time: DateTime::from_timestamp_millis(session_time_updated),
                model,
                reasoning_effort,
                project_path,
                project_name,
                is_active: false,
                token_totals: TokenTotals::default(),
            });
        }

        let Ok(message) = serde_json::from_str::<Value>(&message_data) else {
            continue;
        };
        if opencode_message_role(&message) != Some("assistant") {
            continue;
        }

        if let Some(session) = current_session.as_mut() {
            session.model = opencode_model(&message).unwrap_or_else(|| session.model.clone());
            session.reasoning_effort =
                opencode_reasoning_effort(&message).or(session.reasoning_effort.take());
            if let Some(tokens) = message.get("tokens") {
                add_opencode_tokens(&mut session.token_totals, tokens);
            } else if let Some(usage) = message.get("usage") {
                add_opencode_usage(&mut session.token_totals, usage);
            }
        }
    }

    if let Some(session) = current_session
        && session.token_totals.total_tokens.is_some()
    {
        sessions.push(session);
    }

    Ok(sessions)
}

fn add_opencode_tokens(token_totals: &mut TokenTotals, tokens: &Value) {
    let non_cached_input_tokens = get_u64(tokens, "input");
    let cache_read_tokens = tokens
        .get("cache")
        .and_then(|cache| get_u64(cache, "read"))
        .unwrap_or(0);

    add_optional_u64(
        &mut token_totals.input_tokens,
        non_cached_input_tokens.map(|input_tokens| input_tokens + cache_read_tokens),
    );
    add_optional_u64(
        &mut token_totals.non_cached_input_tokens,
        non_cached_input_tokens,
    );
    add_optional_u64(&mut token_totals.output_tokens, get_u64(tokens, "output"));
    add_optional_u64(
        &mut token_totals.reasoning_output_tokens,
        get_u64(tokens, "reasoning"),
    );
    if let Some(cache) = tokens.get("cache") {
        add_optional_u64(&mut token_totals.cache_read_tokens, get_u64(cache, "read"));
        add_optional_u64(
            &mut token_totals.cache_write_tokens,
            get_u64(cache, "write"),
        );
    }
    add_optional_u64(
        &mut token_totals.total_tokens,
        get_u64(tokens, "total").or_else(|| {
            match (get_u64(tokens, "input"), get_u64(tokens, "output")) {
                (Some(input_tokens), Some(output_tokens)) => Some(input_tokens + output_tokens),
                _ => None,
            }
        }),
    );
}

fn add_optional_u64(total: &mut Option<u64>, value: Option<u64>) {
    if let Some(value) = value {
        *total = Some(total.unwrap_or(0) + value);
    }
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

fn project_name_from_path(project_path: Option<&Path>) -> Option<String> {
    project_path
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(str::to_owned)
}

fn source_recorded_session_name(record: &Value) -> Option<String> {
    ["session_name", "sessionName", "name", "title"]
        .into_iter()
        .find_map(|field_name| record.get(field_name).and_then(Value::as_str))
        .and_then(non_empty_string)
}

fn non_empty_string(text: &str) -> Option<String> {
    let trimmed_text = text.trim();
    (!trimmed_text.is_empty()).then(|| trimmed_text.to_owned())
}

fn parse_time_value(value: &Value) -> Option<DateTime<Utc>> {
    value.as_str()?.parse::<DateTime<Utc>>().ok()
}

fn source_modified_time(source_path: &Path) -> Option<DateTime<Utc>> {
    fs::metadata(source_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(DateTime::<Utc>::from)
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
                r#"{"timestamp":"2026-05-10T10:00:00Z","type":"session_meta","payload":{"timestamp":"2026-05-10T09:59:00Z","cwd":"/home/person/project","name":"Implement Session Name"}}"#,
                r#"{"timestamp":"2026-05-10T10:00:01Z","type":"turn_context","payload":{"model":"gpt-5.5","effort":"low","cwd":"/home/person/project"}}"#,
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
        assert_eq!(codex_session.ai_coding_agent, AiCodingAgent::Codex);
        assert_eq!(
            codex_session.session_name.as_deref(),
            Some("Implement Session Name")
        );
        assert_eq!(codex_session.model, "gpt-5.5");
        assert_eq!(codex_session.reasoning_effort.as_deref(), Some("low"));
        assert_eq!(codex_session.compact_model_label(), "gpt-5.5 · low");
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
    fn parses_pi_session_file_by_summing_assistant_usage() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let session_file_path = temporary_directory.path().join("pi-session.jsonl");
        write_session_file(
            &session_file_path,
            &[
                r#"{"type":"session","version":3,"id":"session-id","timestamp":"2026-05-10T09:59:00Z","cwd":"/home/person/project","name":"Pi Session Name"}"#,
                r#"{"type":"message","id":"first","parentId":null,"timestamp":"2026-05-10T10:00:00Z","message":{"role":"assistant","provider":"anthropic","model":"claude-sonnet-4-5","usage":{"input":1000,"output":300,"cacheRead":250,"cacheWrite":100,"totalTokens":1300,"cost":{"total":0.01}},"stopReason":"stop","content":[]}}"#,
                r#"{"type":"model_change","id":"model","parentId":"first","timestamp":"2026-05-10T10:01:00Z","provider":"anthropic","modelId":"claude-opus-4-5"}"#,
                r#"{"type":"thinking_level_change","id":"thinking","parentId":"model","timestamp":"2026-05-10T10:01:30Z","thinkingLevel":"high"}"#,
                r#"{"type":"message","id":"second","parentId":"thinking","timestamp":"2026-05-10T10:02:00Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-4-5","usage":{"input":500,"output":200,"cacheRead":50,"cacheWrite":25,"totalTokens":700,"cost":{"total":0.02}},"stopReason":"stop","content":[]}}"#,
            ],
        );

        let parsed_usage = read_codex_sessions(&UsageSourceResolution::Readable {
            path: session_file_path,
            is_custom: true,
        })
        .expect("parsed usage");

        assert_eq!(parsed_usage.codex_sessions.len(), 1);
        let pi_session = &parsed_usage.codex_sessions[0];
        assert_eq!(pi_session.ai_coding_agent, AiCodingAgent::Pi);
        assert_eq!(pi_session.session_name.as_deref(), Some("Pi Session Name"));
        assert_eq!(pi_session.model, "claude-opus-4-5");
        assert_eq!(pi_session.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(pi_session.compact_model_label(), "claude-opus-4-5 · high");
        assert_eq!(pi_session.project_name.as_deref(), Some("project"));
        assert_eq!(pi_session.token_totals.input_tokens, Some(1500));
        assert_eq!(pi_session.token_totals.output_tokens, Some(500));
        assert_eq!(pi_session.token_totals.cache_read_tokens, Some(300));
        assert_eq!(pi_session.token_totals.cache_write_tokens, Some(125));
        assert_eq!(pi_session.token_totals.non_cached_input_tokens, Some(1200));
        assert_eq!(pi_session.token_totals.total_tokens, Some(2000));
    }

    #[test]
    fn parses_opencode_json_file_by_summing_assistant_usage() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let session_file_path = temporary_directory.path().join("opencode-session.json");
        fs::write(
            &session_file_path,
            r#"{"id":"ses_123","time":{"created":"2026-05-10T09:59:00Z"},"cwd":"/home/person/project","title":"OpenCode Session Name","message":{"role":"assistant","modelID":"openai/gpt-5.5","reasoningEffort":"medium","usage":{"input":1000,"output":300,"cacheRead":250,"cacheWrite":100,"reasoning":20,"totalTokens":1300}}}"#,
        )
        .expect("OpenCode session file");

        let parsed_usage = read_codex_sessions(&UsageSourceResolution::Readable {
            path: session_file_path,
            is_custom: true,
        })
        .expect("parsed usage");

        assert_eq!(parsed_usage.codex_sessions.len(), 1);
        let opencode_session = &parsed_usage.codex_sessions[0];
        assert_eq!(opencode_session.ai_coding_agent, AiCodingAgent::Opencode);
        assert_eq!(
            opencode_session.session_name.as_deref(),
            Some("OpenCode Session Name")
        );
        assert_eq!(opencode_session.model, "openai/gpt-5.5");
        assert_eq!(opencode_session.reasoning_effort.as_deref(), Some("medium"));
        assert_eq!(opencode_session.project_name.as_deref(), Some("project"));
        assert_eq!(opencode_session.token_totals.input_tokens, Some(1000));
        assert_eq!(opencode_session.token_totals.output_tokens, Some(300));
        assert_eq!(opencode_session.token_totals.cache_read_tokens, Some(250));
        assert_eq!(opencode_session.token_totals.cache_write_tokens, Some(100));
        assert_eq!(
            opencode_session.token_totals.reasoning_output_tokens,
            Some(20)
        );
        assert_eq!(
            opencode_session.token_totals.non_cached_input_tokens,
            Some(750)
        );
        assert_eq!(opencode_session.token_totals.total_tokens, Some(1300));
    }

    #[test]
    fn parses_opencode_database_by_summing_assistant_messages() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let database_path = temporary_directory.path().join("opencode.db");
        let connection = Connection::open(&database_path).expect("OpenCode database");
        connection
            .execute_batch(
                r#"
                create table session (
                    id text primary key,
                    time_created integer not null,
                    time_updated integer not null,
                    directory text not null,
                    title text not null,
                    model text
                );
                create table message (
                    id text primary key,
                    session_id text not null,
                    time_created integer not null,
                    data text not null
                );
                insert into session values (
                    'ses_123',
                    1778389763116,
                    1778393363116,
                    '/home/person/project',
                    'OpenCode Database Session',
                    '{"id":"gpt-5.5-fast","providerID":"openai","variant":"high"}'
                );
                insert into message values (
                    'msg_user',
                    'ses_123',
                    1778389763142,
                    '{"role":"user","model":{"providerID":"openai","modelID":"gpt-5.4-mini-fast"}}'
                );
                insert into message values (
                    'msg_assistant',
                    'ses_123',
                    1778389763188,
                    '{"role":"assistant","modelID":"gpt-5.5-fast","tokens":{"total":14925,"input":14776,"output":16,"reasoning":133,"cache":{"write":10,"read":200}}}'
                );
                "#,
            )
            .expect("OpenCode schema and rows");

        let parsed_usage = read_codex_sessions(&UsageSourceResolution::Readable {
            path: temporary_directory.path().to_path_buf(),
            is_custom: true,
        })
        .expect("parsed usage");

        assert_eq!(parsed_usage.codex_sessions.len(), 1);
        let opencode_session = &parsed_usage.codex_sessions[0];
        assert_eq!(opencode_session.ai_coding_agent, AiCodingAgent::Opencode);
        assert_eq!(
            opencode_session.session_name.as_deref(),
            Some("OpenCode Database Session")
        );
        assert_eq!(
            opencode_session.session_start_time,
            DateTime::from_timestamp_millis(1778389763116).expect("session start time")
        );
        assert_eq!(
            opencode_session.session_last_modified_time,
            DateTime::from_timestamp_millis(1778393363116)
        );
        assert_eq!(opencode_session.model, "gpt-5.5-fast");
        assert_eq!(opencode_session.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(
            opencode_session.compact_model_label(),
            "gpt-5.5-fast · high"
        );
        assert_eq!(opencode_session.project_name.as_deref(), Some("project"));
        assert_eq!(opencode_session.token_totals.input_tokens, Some(14976));
        assert_eq!(opencode_session.token_totals.output_tokens, Some(16));
        assert_eq!(
            opencode_session.token_totals.reasoning_output_tokens,
            Some(133)
        );
        assert_eq!(opencode_session.token_totals.cache_read_tokens, Some(200));
        assert_eq!(opencode_session.token_totals.cache_write_tokens, Some(10));
        assert_eq!(
            opencode_session.token_totals.non_cached_input_tokens,
            Some(14776)
        );
        assert_eq!(opencode_session.token_totals.total_tokens, Some(14925));
    }

    #[test]
    fn default_source_includes_opencode_database_next_to_codex_sessions() {
        let temporary_home = tempfile::tempdir().expect("temporary home");
        let codex_sessions_directory = temporary_home.path().join("codex-sessions");
        let opencode_directory = temporary_home.path().join("opencode");
        fs::create_dir_all(&codex_sessions_directory).expect("Codex sessions directory");
        fs::create_dir_all(&opencode_directory).expect("OpenCode directory");
        let codex_session_file_path = codex_sessions_directory.join("codex-session.jsonl");
        write_session_file(
            &codex_session_file_path,
            &[
                r#"{"timestamp":"2026-05-10T10:00:00Z","type":"session_meta","payload":{"timestamp":"2026-05-10T10:00:00Z","cwd":"/home/person/codex-project"}}"#,
                r#"{"timestamp":"2026-05-10T10:00:01Z","type":"turn_context","payload":{"model":"gpt-5.5"}}"#,
                r#"{"timestamp":"2026-05-10T10:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"output_tokens":20,"total_tokens":30}}}}"#,
            ],
        );
        write_opencode_database(&opencode_directory.join("opencode.db"));

        unsafe {
            std::env::set_var("PI_HOME", temporary_home.path().join("missing-pi"));
            std::env::set_var("OPENCODE_HOME", &opencode_directory);
        }

        let parsed_usage = read_codex_sessions(&UsageSourceResolution::Readable {
            path: codex_sessions_directory,
            is_custom: false,
        })
        .expect("parsed usage");

        assert_eq!(parsed_usage.codex_sessions.len(), 2);
        assert!(
            parsed_usage
                .codex_sessions
                .iter()
                .any(|session| session.ai_coding_agent == AiCodingAgent::Codex)
        );
        assert!(
            parsed_usage
                .codex_sessions
                .iter()
                .any(|session| session.ai_coding_agent == AiCodingAgent::Opencode)
        );
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

    fn write_opencode_database(database_path: &Path) {
        let connection = Connection::open(database_path).expect("OpenCode database");
        connection
            .execute_batch(
                r#"
                create table session (
                    id text primary key,
                    time_created integer not null,
                    time_updated integer not null,
                    directory text not null,
                    title text not null,
                    model text
                );
                create table message (
                    id text primary key,
                    session_id text not null,
                    time_created integer not null,
                    data text not null
                );
                insert into session values (
                    'ses_123',
                    1778389763116,
                    1778393363116,
                    '/home/person/opencode-project',
                    'OpenCode Default Session',
                    null
                );
                insert into message values (
                    'msg_assistant',
                    'ses_123',
                    1778389763188,
                    '{"role":"assistant","modelID":"gpt-5.4-mini-fast","tokens":{"total":14925,"input":14776,"output":16,"reasoning":133,"cache":{"write":10,"read":200}}}'
                );
                "#,
            )
            .expect("OpenCode schema and rows");
    }
}
