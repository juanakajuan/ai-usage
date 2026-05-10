use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

#[test]
fn json_output_redacts_paths_and_uses_custom_usage_source() {
    let temporary_directory = tempfile::tempdir().expect("temporary directory");
    let codex_home = temporary_directory.path().join("codex-home");
    let usage_source = codex_home.join("sessions");
    std::fs::create_dir_all(&usage_source).expect("usage source directory");
    write_session_file(
        &usage_source.join("session.jsonl"),
        &[
            r#"{"timestamp":"2026-05-10T10:00:00Z","type":"session_meta","payload":{"timestamp":"2026-05-10T10:00:00Z","cwd":"/home/person/project"}}"#,
            r#"{"timestamp":"2026-05-10T10:00:01Z","type":"turn_context","payload":{"model":"gpt-5.5"}}"#,
            r#"{"timestamp":"2026-05-10T10:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000000,"cached_input_tokens":250000,"output_tokens":500000,"total_tokens":1500000}}}}"#,
            r#"{"timestamp":"2026-05-10T10:00:03Z","type":"event_msg","payload":{"type":"task_complete"}}"#,
        ],
    );

    let output = Command::new(env!("CARGO_BIN_EXE_ai-usage"))
        .arg("--json")
        .arg("--redact-paths")
        .arg("--usage-source")
        .arg(&usage_source)
        .env("HOME", temporary_directory.path())
        .output()
        .expect("process output");

    assert!(output.status.success());
    let json_output: Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(json_output["output_schema_version"], 1);
    assert_eq!(json_output["usage_source"]["is_custom"], true);
    assert_eq!(json_output["usage_source"]["path"], "~/codex-home/sessions");
    assert_eq!(
        json_output["headline_periods"][3]["session_detail"][0]["source_path"],
        "~/codex-home/sessions/session.jsonl"
    );
}

#[test]
fn missing_usage_source_is_reported_in_json_without_process_failure() {
    let temporary_directory = tempfile::tempdir().expect("temporary directory");
    let missing_usage_source = temporary_directory.path().join("missing-sessions");

    let output = Command::new(env!("CARGO_BIN_EXE_ai-usage"))
        .arg("--json")
        .arg("--usage-source")
        .arg(&missing_usage_source)
        .env("HOME", temporary_directory.path())
        .output()
        .expect("process output");

    assert!(output.status.success());
    let json_output: Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(json_output["usage_source"]["is_readable"], false);
    assert_eq!(
        json_output["headline_periods"][0]["period_cost_state"]["kind"],
        "missing_usage_source"
    );
}

#[test]
fn non_interactive_terminal_startup_renders_summary_smoke_output() {
    let temporary_directory = tempfile::tempdir().expect("temporary directory");
    let usage_source = temporary_directory.path().join("sessions");
    std::fs::create_dir_all(&usage_source).expect("usage source directory");
    write_session_file(
        &usage_source.join("session.jsonl"),
        &[
            r#"{"timestamp":"2026-05-10T10:00:00Z","type":"session_meta","payload":{"timestamp":"2026-05-10T10:00:00Z","cwd":"/home/person/project"}}"#,
            r#"{"timestamp":"2026-05-10T10:00:01Z","type":"turn_context","payload":{"model":"gpt-5.5"}}"#,
            r#"{"timestamp":"2026-05-10T10:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}}}}"#,
        ],
    );

    let output = Command::new(env!("CARGO_BIN_EXE_ai-usage"))
        .arg("--usage-source")
        .arg(&usage_source)
        .env("HOME", temporary_directory.path())
        .output()
        .expect("process output");

    assert!(output.status.success());
    let terminal_output = String::from_utf8(output.stdout).expect("terminal output");
    assert!(terminal_output.contains("AI Usage"));
    assert!(terminal_output.contains("Session Detail"));
    assert!(terminal_output.contains("tokens 15"));
}

fn write_session_file(path: &Path, lines: &[&str]) {
    let mut file = File::create(path).expect("session file");
    for line in lines {
        writeln!(file, "{line}").expect("session line");
    }
}
