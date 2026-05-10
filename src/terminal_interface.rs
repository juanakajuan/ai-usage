//! Ratatui and Crossterm terminal presentation.

use std::error::Error;
use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::cost::{CostState, PricedCodexSession, format_united_states_dollar_cost};
use crate::reporting_period::{
    DerivedSummary, HeadlinePeriod, PeriodCostState, ReportingPeriodKind,
};

/// Terminal actions requested by keyboard input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalAction {
    /// Keep running the terminal interface.
    Continue,
    /// Reload Derived Summary from Current Source State.
    Reload,
    /// Quit the terminal interface cleanly.
    Quit,
}

/// Mutable terminal navigation and inspection state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalInterfaceState {
    /// Selected Headline Period index.
    pub selected_headline_period_index: usize,
    /// Selected Session Detail index within the selected period.
    pub selected_session_index: usize,
    /// Whether Expanded Session Detail is open.
    pub is_expanded_session_detail_open: bool,
}

impl TerminalInterfaceState {
    /// Creates default terminal state focused on the All-Time period.
    pub fn new(derived_summary: &DerivedSummary) -> Self {
        let selected_headline_period_index = derived_summary
            .headline_periods
            .iter()
            .position(|period| period.kind == ReportingPeriodKind::AllTime)
            .unwrap_or(0);

        Self {
            selected_headline_period_index,
            selected_session_index: 0,
            is_expanded_session_detail_open: false,
        }
    }

    /// Applies one key press and returns the requested terminal action.
    pub fn handle_key_code(
        &mut self,
        key_code: KeyCode,
        derived_summary: &DerivedSummary,
    ) -> TerminalAction {
        match key_code {
            KeyCode::Char('q') | KeyCode::Esc => TerminalAction::Quit,
            KeyCode::Char('r') => TerminalAction::Reload,
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_session_selection(1, derived_summary);
                TerminalAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_session_selection(-1, derived_summary);
                TerminalAction::Continue
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.move_period_selection(1, derived_summary);
                TerminalAction::Continue
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.move_period_selection(-1, derived_summary);
                TerminalAction::Continue
            }
            KeyCode::Enter => {
                if self.selected_session(derived_summary).is_some() {
                    self.is_expanded_session_detail_open = true;
                }
                TerminalAction::Continue
            }
            _ => TerminalAction::Continue,
        }
    }

    /// Keeps selection indexes valid after a Derived Summary reload.
    pub fn reconcile_with_summary(&mut self, derived_summary: &DerivedSummary) {
        if derived_summary.headline_periods.is_empty() {
            self.selected_headline_period_index = 0;
            self.selected_session_index = 0;
            self.is_expanded_session_detail_open = false;
            return;
        }

        self.selected_headline_period_index = self
            .selected_headline_period_index
            .min(derived_summary.headline_periods.len() - 1);
        let session_count = self
            .selected_period(derived_summary)
            .map(|period| period.session_detail.len())
            .unwrap_or(0);
        if session_count == 0 {
            self.selected_session_index = 0;
            self.is_expanded_session_detail_open = false;
        } else {
            self.selected_session_index = self.selected_session_index.min(session_count - 1);
        }
    }

    fn move_period_selection(&mut self, direction: isize, derived_summary: &DerivedSummary) {
        if derived_summary.headline_periods.is_empty() {
            return;
        }
        let last_index = derived_summary.headline_periods.len() - 1;
        self.selected_headline_period_index =
            offset_index(self.selected_headline_period_index, direction, last_index);
        self.selected_session_index = 0;
        self.is_expanded_session_detail_open = false;
    }

    fn move_session_selection(&mut self, direction: isize, derived_summary: &DerivedSummary) {
        let Some(selected_period) = self.selected_period(derived_summary) else {
            return;
        };
        if selected_period.session_detail.is_empty() {
            self.selected_session_index = 0;
            self.is_expanded_session_detail_open = false;
            return;
        }
        let last_index = selected_period.session_detail.len() - 1;
        self.selected_session_index =
            offset_index(self.selected_session_index, direction, last_index);
        self.is_expanded_session_detail_open = false;
    }

    fn selected_period<'a>(
        &self,
        derived_summary: &'a DerivedSummary,
    ) -> Option<&'a HeadlinePeriod> {
        derived_summary
            .headline_periods
            .get(self.selected_headline_period_index)
    }

    fn selected_session<'a>(
        &self,
        derived_summary: &'a DerivedSummary,
    ) -> Option<&'a PricedCodexSession> {
        self.selected_period(derived_summary)?
            .session_detail
            .get(self.selected_session_index)
    }
}

/// Runs the interactive terminal interface.
pub fn run_terminal_interface(
    initial_derived_summary: DerivedSummary,
    mut reload_derived_summary: impl FnMut() -> Result<DerivedSummary, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_event_loop(
        &mut terminal,
        initial_derived_summary,
        &mut reload_derived_summary,
    );

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    mut derived_summary: DerivedSummary,
    reload_derived_summary: &mut impl FnMut() -> Result<DerivedSummary, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let mut terminal_interface_state = TerminalInterfaceState::new(&derived_summary);
    loop {
        terminal.draw(|frame| {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(8), Constraint::Min(4)])
                .split(frame.area());
            frame.render_widget(summary_paragraph(&derived_summary), layout[0]);
            frame.render_widget(
                session_panel(&derived_summary, &terminal_interface_state),
                layout[1],
            );
        })?;

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key_event) = event::read()?
        {
            match terminal_interface_state.handle_key_code(key_event.code, &derived_summary) {
                TerminalAction::Continue => {}
                TerminalAction::Reload => {
                    derived_summary = reload_derived_summary()?;
                    terminal_interface_state.reconcile_with_summary(&derived_summary);
                }
                TerminalAction::Quit => break,
            }
        }
    }

    Ok(())
}

/// Renders a non-interactive terminal smoke-check summary.
pub fn render_terminal_summary(derived_summary: &DerivedSummary) -> String {
    let mut lines = vec!["AI Usage".to_owned()];
    for headline_period in &derived_summary.headline_periods {
        lines.push(render_headline_period_row(
            headline_period,
            largest_period_cost(derived_summary),
        ));
    }
    if !derived_summary.data_quality_notices.is_empty() {
        lines.push(format!(
            "! Data Quality: {} notice(s), first: {}",
            derived_summary.data_quality_notices.len(),
            derived_summary.data_quality_notices[0].detail
        ));
    }
    if let Some(all_time_period) = selected_period(derived_summary, &ReportingPeriodKind::AllTime) {
        lines.push(String::new());
        lines.push("Session Detail".to_owned());
        lines.extend(render_session_detail_lines(all_time_period, 8));
    }
    if let Some(first_session) = selected_period(derived_summary, &ReportingPeriodKind::AllTime)
        .and_then(|period| period.session_detail.first())
    {
        lines.push(String::new());
        lines.push("Expanded Session Detail".to_owned());
        lines.extend(render_expanded_session_detail_lines(first_session));
    }
    if !derived_summary.all_time_detail.is_empty() {
        lines.push(String::new());
        lines.push("All-Time Detail".to_owned());
        for monthly_session_group in &derived_summary.all_time_detail {
            lines.push(format!(
                "{}  {} session(s)",
                monthly_session_group.local_month_start_date,
                monthly_session_group.session_detail.len()
            ));
        }
    }
    lines.join("\n")
}

fn summary_paragraph(derived_summary: &DerivedSummary) -> Paragraph<'_> {
    let mut lines = vec![Line::from(Span::styled(
        "AI Usage",
        Style::default()
            .fg(Color::Rgb(224, 214, 194))
            .add_modifier(Modifier::BOLD),
    ))];
    let largest_cost = largest_period_cost(derived_summary);
    for headline_period in &derived_summary.headline_periods {
        lines.push(Line::from(render_headline_period_row(
            headline_period,
            largest_cost,
        )));
    }
    if !derived_summary.data_quality_notices.is_empty() {
        lines.push(Line::from(format!(
            "! Data Quality: {} notice(s)",
            derived_summary.data_quality_notices.len()
        )));
    }

    Paragraph::new(lines)
        .block(Block::default().borders(Borders::BOTTOM))
        .style(
            Style::default()
                .bg(Color::Rgb(14, 14, 13))
                .fg(Color::Rgb(224, 214, 194)),
        )
}

fn session_panel(
    derived_summary: &DerivedSummary,
    terminal_interface_state: &TerminalInterfaceState,
) -> Paragraph<'static> {
    if terminal_interface_state.is_expanded_session_detail_open
        && let Some(priced_session) = terminal_interface_state.selected_session(derived_summary)
    {
        return Paragraph::new(render_expanded_session_detail_lines(priced_session).join("\n"))
            .block(
                Block::default()
                    .title("Expanded Session Detail")
                    .borders(Borders::TOP),
            )
            .style(matte_box_style());
    }

    Paragraph::new(session_list_text(derived_summary, terminal_interface_state))
        .block(
            Block::default()
                .title("Session Detail")
                .borders(Borders::TOP),
        )
        .style(matte_box_style())
}

fn session_list_text(
    derived_summary: &DerivedSummary,
    terminal_interface_state: &TerminalInterfaceState,
) -> String {
    let sessions = terminal_interface_state
        .selected_period(derived_summary)
        .map(|period| period.session_detail.as_slice())
        .unwrap_or(&[]);
    let rows = sessions
        .iter()
        .take(20)
        .enumerate()
        .map(|(session_index, priced_session)| {
            let project_name = priced_session
                .codex_session
                .project_name
                .as_deref()
                .unwrap_or("(no project)");
            let selection_marker =
                if session_index == terminal_interface_state.selected_session_index {
                    ">"
                } else {
                    " "
                };
            format!(
                "{selection_marker} {}",
                compact_session_row(priced_session, project_name)
            )
        });

    rows.collect::<Vec<_>>().join("\n")
}

fn matte_box_style() -> Style {
    Style::default()
        .bg(Color::Rgb(14, 14, 13))
        .fg(Color::Rgb(224, 214, 194))
}

/// Renders newest-first Session Detail lines for a selected Reporting Period.
pub fn render_session_detail_lines(
    headline_period: &HeadlinePeriod,
    maximum_session_count: usize,
) -> Vec<String> {
    headline_period
        .session_detail
        .iter()
        .take(maximum_session_count)
        .map(|priced_session| {
            let project_name = priced_session
                .codex_session
                .project_name
                .as_deref()
                .unwrap_or("(no project)");
            compact_session_row(priced_session, project_name)
        })
        .collect()
}

/// Renders an expanded terminal detail view for one Codex Session.
pub fn render_expanded_session_detail_lines(priced_session: &PricedCodexSession) -> Vec<String> {
    let token_totals = &priced_session.codex_session.token_totals;
    let project_path = priced_session
        .codex_session
        .project_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "(no project path)".to_owned());
    let price_schedule = priced_session
        .price_schedule_match
        .as_ref()
        .map(|price_schedule_match| {
            format!(
                "{} effective {}",
                price_schedule_match.model, price_schedule_match.effective_date
            )
        })
        .unwrap_or_else(|| "no price schedule match".to_owned());

    let mut lines = vec![
        format!("Project Path: {project_path}"),
        format!("Model: {}", priced_session.codex_session.model),
        format!("Price Schedule: {price_schedule}"),
        format!(
            "Input Tokens: {}",
            optional_token_count(token_totals.input_tokens)
        ),
        format!(
            "Non-Cached Input Tokens: {}",
            optional_token_count(token_totals.non_cached_input_tokens)
        ),
        format!(
            "Cache Read Tokens: {}",
            optional_token_count(token_totals.cache_read_tokens)
        ),
        format!(
            "Cache Write Tokens: {}",
            optional_token_count(token_totals.cache_write_tokens)
        ),
        format!(
            "Output Tokens: {}",
            optional_token_count(token_totals.output_tokens)
        ),
        format!(
            "Reasoning Output Tokens: {}",
            optional_token_count(token_totals.reasoning_output_tokens)
        ),
        format!(
            "Total Tokens: {}",
            optional_token_count(token_totals.total_tokens)
        ),
    ];

    match &priced_session.cost_state {
        CostState::Complete { .. } => lines.push("Incomplete Reasons: none".to_owned()),
        CostState::Partial { reasons, .. } | CostState::Incomplete { reasons } => {
            lines.push(format!("Incomplete Reasons: {}", reasons.len()));
            lines.extend(reasons.iter().map(|reason| format!("- {reason:?}")));
        }
    }

    lines
}

fn render_headline_period_row(
    headline_period: &HeadlinePeriod,
    largest_period_cost: rust_decimal::Decimal,
) -> String {
    format!(
        "{} {}  {}  tokens {}  sessions {}  trend {}",
        period_status_marker(&headline_period.summary_totals.period_cost_state),
        reporting_period_label(&headline_period.kind),
        format_united_states_dollar_cost(
            headline_period
                .summary_totals
                .known_united_states_dollar_cost
        ),
        optional_token_count(headline_period.summary_totals.token_totals.total_tokens),
        headline_period.summary_totals.session_count,
        trend_bar(
            headline_period
                .summary_totals
                .known_united_states_dollar_cost,
            largest_period_cost
        )
    )
}

fn compact_session_row(priced_session: &PricedCodexSession, project_name: &str) -> String {
    format!(
        "{} {}  {}  {}  {}  tokens {}",
        session_status_marker(priced_session),
        priced_session
            .codex_session
            .session_start_time
            .format("%Y-%m-%d %H:%M"),
        priced_session.codex_session.model,
        session_cost_label(&priced_session.cost_state),
        project_name,
        optional_token_count(priced_session.codex_session.token_totals.total_tokens)
    )
}

fn selected_period<'a>(
    derived_summary: &'a DerivedSummary,
    reporting_period_kind: &ReportingPeriodKind,
) -> Option<&'a HeadlinePeriod> {
    derived_summary
        .headline_periods
        .iter()
        .find(|period| &period.kind == reporting_period_kind)
}

fn offset_index(current_index: usize, direction: isize, last_index: usize) -> usize {
    if direction.is_negative() {
        current_index.saturating_sub(direction.unsigned_abs())
    } else {
        current_index
            .saturating_add(direction as usize)
            .min(last_index)
    }
}

fn largest_period_cost(derived_summary: &DerivedSummary) -> rust_decimal::Decimal {
    derived_summary
        .headline_periods
        .iter()
        .map(|period| period.summary_totals.known_united_states_dollar_cost)
        .max()
        .unwrap_or(rust_decimal::Decimal::ZERO)
}

fn trend_bar(cost: rust_decimal::Decimal, largest_cost: rust_decimal::Decimal) -> String {
    if largest_cost <= rust_decimal::Decimal::ZERO {
        return "[.....]".to_owned();
    }
    let ratio = cost / largest_cost;
    let filled_width = (ratio * rust_decimal::Decimal::from(5u8))
        .round()
        .to_string()
        .parse::<usize>()
        .unwrap_or(0)
        .min(5);
    format!(
        "[{}{}]",
        "#".repeat(filled_width),
        ".".repeat(5 - filled_width)
    )
}

fn optional_token_count(token_count: Option<u64>) -> String {
    token_count
        .map(|token_count| token_count.to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn period_status_marker(period_cost_state: &PeriodCostState) -> &'static str {
    match period_cost_state {
        PeriodCostState::MissingUsageSource => "[!]",
        PeriodCostState::ZeroUsage => "[0]",
        PeriodCostState::Complete => "[+]",
        PeriodCostState::Partial { .. } => "[~]",
        PeriodCostState::Incomplete { .. } => "[?]",
    }
}

fn session_status_marker(priced_session: &PricedCodexSession) -> &'static str {
    if priced_session.codex_session.is_active {
        return "[A]";
    }
    match priced_session.cost_state {
        CostState::Complete { .. } => "[+]",
        CostState::Partial { .. } => "[~]",
        CostState::Incomplete { .. } => "[?]",
    }
}

fn session_cost_label(cost_state: &CostState) -> String {
    match cost_state {
        CostState::Complete {
            united_states_dollar_cost,
        } => format_united_states_dollar_cost(*united_states_dollar_cost),
        CostState::Partial {
            known_united_states_dollar_cost,
            ..
        } => format!(
            "{} partial",
            format_united_states_dollar_cost(*known_united_states_dollar_cost)
        ),
        CostState::Incomplete { .. } => "incomplete".to_owned(),
    }
}

fn reporting_period_label(kind: &ReportingPeriodKind) -> &'static str {
    match kind {
        ReportingPeriodKind::Daily => "Daily",
        ReportingPeriodKind::Weekly => "Weekly",
        ReportingPeriodKind::Monthly => "Monthly",
        ReportingPeriodKind::AllTime => "All-Time",
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;

    use super::*;
    use crate::cost::{IncompleteCostReason, PriceScheduleMatch};
    use crate::reporting_period::{HeadlinePeriod, SummaryTotals, build_derived_summary_at};
    use crate::session::{CodexSession, DataQualityNotice, DataQualityNoticeKind, TokenTotals};
    use crate::usage_source::UsageSourceResolution;

    #[test]
    fn terminal_summary_renders_headline_tokens_markers_trends_and_data_quality() {
        let derived_summary = DerivedSummary {
            usage_source_resolution: UsageSourceResolution::Readable {
                path: "source".into(),
                is_custom: false,
            },
            headline_periods: vec![
                headline_period(
                    ReportingPeriodKind::Daily,
                    PeriodCostState::ZeroUsage,
                    Vec::new(),
                ),
                headline_period(
                    ReportingPeriodKind::Weekly,
                    PeriodCostState::Partial {
                        reasons: vec![IncompleteCostReason::UnpricedUsage {
                            model: "unknown".to_owned(),
                        }],
                    },
                    vec![priced_session(
                        "active",
                        true,
                        CostState::Incomplete { reasons: vec![] },
                    )],
                ),
                headline_period(
                    ReportingPeriodKind::Monthly,
                    PeriodCostState::Incomplete {
                        reasons: vec![IncompleteCostReason::MissingTokenCategory {
                            token_category: "Output Tokens".to_owned(),
                        }],
                    },
                    Vec::new(),
                ),
                headline_period(
                    ReportingPeriodKind::AllTime,
                    PeriodCostState::Complete,
                    vec![priced_session(
                        "active",
                        true,
                        CostState::Incomplete { reasons: vec![] },
                    )],
                ),
            ],
            all_time_detail: Vec::new(),
            data_quality_notices: vec![DataQualityNotice {
                source_path: "bad.jsonl".into(),
                line_number: Some(1),
                kind: DataQualityNoticeKind::ParseProblem,
                detail: "invalid JSONL record".to_owned(),
            }],
        };

        let rendered_summary = render_terminal_summary(&derived_summary);

        assert!(rendered_summary.contains("[0] Daily"));
        assert!(rendered_summary.contains("tokens 15"));
        assert!(rendered_summary.contains("[~] Weekly"));
        assert!(rendered_summary.contains("[?] Monthly"));
        assert!(rendered_summary.contains("[+] All-Time"));
        assert!(rendered_summary.contains("trend ["));
        assert!(rendered_summary.contains("! Data Quality: 1 notice(s)"));
        assert!(rendered_summary.contains("[A]"));
    }

    #[test]
    fn session_detail_and_all_time_detail_render_newest_first_with_expanded_detail() {
        let older_session = priced_session(
            "older",
            false,
            CostState::Complete {
                united_states_dollar_cost: Decimal::from(1),
            },
        );
        let newer_session = priced_session(
            "newer",
            false,
            CostState::Partial {
                known_united_states_dollar_cost: Decimal::from(2),
                reasons: vec![IncompleteCostReason::MissingTokenCategory {
                    token_category: "Output Tokens".to_owned(),
                }],
            },
        );
        let current_local_time = chrono::Local
            .with_ymd_and_hms(2026, 5, 10, 12, 0, 0)
            .unwrap();
        let derived_summary = build_derived_summary_at(
            UsageSourceResolution::Readable {
                path: "source".into(),
                is_custom: false,
            },
            vec![older_session.clone(), newer_session.clone()],
            Vec::new(),
            current_local_time,
        );
        let all_time_period = derived_summary
            .headline_periods
            .iter()
            .find(|period| period.kind == ReportingPeriodKind::AllTime)
            .expect("all-time period");

        let session_detail = render_session_detail_lines(all_time_period, 10);
        let expanded_detail = render_expanded_session_detail_lines(&newer_session);

        assert!(session_detail[0].contains("newer"));
        assert!(session_detail[0].contains("gpt-5.5"));
        assert!(session_detail[0].contains("tokens 15"));
        assert_eq!(
            derived_summary.all_time_detail[0].session_detail[0],
            newer_session
        );
        assert!(
            expanded_detail
                .iter()
                .any(|line| line.contains("Project Path: /tmp/newer"))
        );
        assert!(
            expanded_detail
                .iter()
                .any(|line| line.contains("Price Schedule: gpt-5.5 effective 2026-01-01"))
        );
        assert!(
            expanded_detail
                .iter()
                .any(|line| line.contains("Cache Read Tokens: 2"))
        );
        assert!(
            expanded_detail
                .iter()
                .any(|line| line.contains("Incomplete Reasons: 1"))
        );
    }

    #[test]
    fn keyboard_navigation_supports_arrows_and_terminal_native_keys() {
        let derived_summary = build_derived_summary_at(
            UsageSourceResolution::Readable {
                path: "source".into(),
                is_custom: false,
            },
            vec![
                priced_session(
                    "first",
                    false,
                    CostState::Complete {
                        united_states_dollar_cost: Decimal::from(1),
                    },
                ),
                priced_session(
                    "second",
                    false,
                    CostState::Complete {
                        united_states_dollar_cost: Decimal::from(1),
                    },
                ),
            ],
            Vec::new(),
            chrono::Local
                .with_ymd_and_hms(2026, 5, 10, 12, 0, 0)
                .unwrap(),
        );
        let mut terminal_interface_state = TerminalInterfaceState::new(&derived_summary);

        assert_eq!(
            terminal_interface_state.handle_key_code(KeyCode::Left, &derived_summary),
            TerminalAction::Continue
        );
        assert_eq!(terminal_interface_state.selected_headline_period_index, 2);
        assert_eq!(
            terminal_interface_state.handle_key_code(KeyCode::Char('l'), &derived_summary),
            TerminalAction::Continue
        );
        assert_eq!(terminal_interface_state.selected_headline_period_index, 3);
        assert_eq!(
            terminal_interface_state.handle_key_code(KeyCode::Down, &derived_summary),
            TerminalAction::Continue
        );
        assert_eq!(terminal_interface_state.selected_session_index, 1);
        assert_eq!(
            terminal_interface_state.handle_key_code(KeyCode::Char('k'), &derived_summary),
            TerminalAction::Continue
        );
        assert_eq!(terminal_interface_state.selected_session_index, 0);
        assert_eq!(
            terminal_interface_state.handle_key_code(KeyCode::Char('j'), &derived_summary),
            TerminalAction::Continue
        );
        assert_eq!(terminal_interface_state.selected_session_index, 1);
        assert_eq!(
            terminal_interface_state.handle_key_code(KeyCode::Char('h'), &derived_summary),
            TerminalAction::Continue
        );
        assert_eq!(terminal_interface_state.selected_headline_period_index, 2);
    }

    #[test]
    fn enter_reload_and_quit_keys_have_terminal_actions() {
        let derived_summary = build_derived_summary_at(
            UsageSourceResolution::Readable {
                path: "source".into(),
                is_custom: false,
            },
            vec![priced_session(
                "selected",
                false,
                CostState::Complete {
                    united_states_dollar_cost: Decimal::from(1),
                },
            )],
            Vec::new(),
            chrono::Local
                .with_ymd_and_hms(2026, 5, 10, 12, 0, 0)
                .unwrap(),
        );
        let reloaded_summary = build_derived_summary_at(
            UsageSourceResolution::Readable {
                path: "source".into(),
                is_custom: false,
            },
            Vec::new(),
            Vec::new(),
            chrono::Local
                .with_ymd_and_hms(2026, 5, 10, 12, 0, 0)
                .unwrap(),
        );
        let mut terminal_interface_state = TerminalInterfaceState::new(&derived_summary);

        assert_eq!(
            terminal_interface_state.handle_key_code(KeyCode::Enter, &derived_summary),
            TerminalAction::Continue
        );
        assert!(terminal_interface_state.is_expanded_session_detail_open);
        assert_eq!(
            terminal_interface_state.handle_key_code(KeyCode::Char('r'), &derived_summary),
            TerminalAction::Reload
        );

        terminal_interface_state.reconcile_with_summary(&reloaded_summary);

        assert_eq!(terminal_interface_state.selected_session_index, 0);
        assert!(!terminal_interface_state.is_expanded_session_detail_open);
        assert_eq!(
            terminal_interface_state.handle_key_code(KeyCode::Char('q'), &reloaded_summary),
            TerminalAction::Quit
        );
        assert_eq!(
            terminal_interface_state.handle_key_code(KeyCode::Esc, &reloaded_summary),
            TerminalAction::Quit
        );
    }

    fn headline_period(
        kind: ReportingPeriodKind,
        period_cost_state: PeriodCostState,
        session_detail: Vec<PricedCodexSession>,
    ) -> HeadlinePeriod {
        HeadlinePeriod {
            kind,
            local_start_date: None,
            local_end_date: None,
            summary_totals: SummaryTotals {
                known_united_states_dollar_cost: Decimal::from(2),
                period_cost_state,
                token_totals: TokenTotals {
                    input_tokens: Some(10),
                    output_tokens: Some(5),
                    total_tokens: Some(15),
                    ..TokenTotals::default()
                },
                session_count: session_detail.len(),
            },
            session_detail,
        }
    }

    fn priced_session(
        project_name: &str,
        is_active: bool,
        cost_state: CostState,
    ) -> PricedCodexSession {
        let minute = if project_name == "newer" { 2 } else { 1 };
        PricedCodexSession {
            codex_session: CodexSession {
                source_path: format!("{project_name}.jsonl").into(),
                session_start_time: Utc.with_ymd_and_hms(2026, 5, 10, 10, minute, 0).unwrap(),
                model: "gpt-5.5".to_owned(),
                project_path: Some(format!("/tmp/{project_name}").into()),
                project_name: Some(project_name.to_owned()),
                is_active,
                token_totals: TokenTotals {
                    input_tokens: Some(10),
                    non_cached_input_tokens: Some(8),
                    cache_read_tokens: Some(2),
                    output_tokens: Some(5),
                    total_tokens: Some(15),
                    ..TokenTotals::default()
                },
            },
            cost_state,
            price_schedule_match: Some(PriceScheduleMatch {
                model: "gpt-5.5".to_owned(),
                effective_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            }),
        }
    }
}
