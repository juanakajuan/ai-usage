//! Ratatui and Crossterm terminal presentation.

use std::error::Error;
use std::io::{self, Stdout};
use std::time::Duration;

use chrono::Local;
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use rust_decimal::Decimal;

use crate::cost::{
    CostState, PriceScheduleMatch, PricedCodexSession, format_united_states_dollar_cost,
};
use crate::reporting_period::{DerivedSummary, HeadlinePeriod, ReportingPeriodKind};

const MATTE_BOX_BACKGROUND: Color = Color::Rgb(14, 14, 13);
const MATTE_BOX_FOREGROUND: Color = Color::Rgb(224, 214, 194);
const MATTE_BOX_MUTED_FOREGROUND: Color = Color::Rgb(139, 132, 116);
const MATTE_BOX_ACCENT: Color = Color::Rgb(117, 172, 154);
const MATTE_BOX_SUCCESS: Color = Color::Rgb(134, 174, 124);
const MATTE_BOX_WARNING: Color = Color::Rgb(211, 151, 98);
const MATTE_BOX_SELECTED_BACKGROUND: Color = Color::Rgb(37, 37, 34);
const SESSION_TABLE_COLUMN_SPACING: u16 = 1;
const SESSION_TABLE_MINIMUM_COLUMN_WIDTHS: [u16; 8] = [6, 7, 10, 6, 14, 7, 25, 12];

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
            _ => TerminalAction::Continue,
        }
    }

    /// Keeps selection indexes valid after a Derived Summary reload.
    pub fn reconcile_with_summary(&mut self, derived_summary: &DerivedSummary) {
        if derived_summary.headline_periods.is_empty() {
            self.selected_headline_period_index = 0;
            self.selected_session_index = 0;
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
    }

    fn move_session_selection(&mut self, direction: isize, derived_summary: &DerivedSummary) {
        let Some(selected_period) = self.selected_period(derived_summary) else {
            return;
        };
        if selected_period.session_detail.is_empty() {
            self.selected_session_index = 0;
            return;
        }
        let last_index = selected_period.session_detail.len() - 1;
        self.selected_session_index =
            offset_index(self.selected_session_index, direction, last_index);
    }

    fn selected_period<'a>(
        &self,
        derived_summary: &'a DerivedSummary,
    ) -> Option<&'a HeadlinePeriod> {
        derived_summary
            .headline_periods
            .get(self.selected_headline_period_index)
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
            render_terminal_screen(frame, &derived_summary, &terminal_interface_state);
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
    let mut lines = vec![
        "AI Usage".to_owned(),
        format!(
            "Updated {}                                             {}",
            Local::now().format("%-I:%M%P"),
            data_quality_summary_label(derived_summary.data_quality_notices.len())
        ),
    ];
    for headline_period in &derived_summary.headline_periods {
        lines.push(render_headline_period_row(headline_period));
    }
    if let Some(all_time_period) = selected_period(derived_summary, &ReportingPeriodKind::AllTime) {
        lines.push(String::new());
        lines.push("Sessions".to_owned());
        lines.push(session_table_header());
        lines.push(session_table_separator());
        lines.extend(render_session_detail_lines(all_time_period, 8));
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

/// Renders the full interactive terminal frame, including the outer panel and key strip.
fn render_terminal_screen(
    frame: &mut Frame<'_>,
    derived_summary: &DerivedSummary,
    terminal_interface_state: &TerminalInterfaceState,
) {
    let screen_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(1)])
        .split(frame.area());
    let content_area = screen_layout[0];
    let footer_area = screen_layout[1];
    let outer_block = Block::default()
        .title(Line::from(" AI Usage ").style(title_style()))
        .borders(Borders::ALL)
        .border_style(border_style())
        .style(matte_box_style());
    let inner_area = outer_block.inner(content_area);

    frame.render_widget(outer_block, content_area);
    render_usage_content(frame, inner_area, derived_summary, terminal_interface_state);
    frame.render_widget(footer_paragraph(), footer_area);
}

/// Renders the Usage Summary and Session Detail content inside the outer panel.
fn render_usage_content(
    frame: &mut Frame<'_>,
    content_area: Rect,
    derived_summary: &DerivedSummary,
    terminal_interface_state: &TerminalInterfaceState,
) {
    if content_area.height == 0 {
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Min(3),
        ])
        .split(content_area);

    render_screen_header(frame, layout[0], derived_summary);
    frame.render_widget(horizontal_separator(content_area.width), layout[1]);
    frame.render_widget(
        summary_table(derived_summary, terminal_interface_state),
        layout[2],
    );
    frame.render_widget(
        session_panel(derived_summary, terminal_interface_state, layout[3]),
        layout[3],
    );
}

/// Renders the updated timestamp and concise Data Quality Notice count.
fn render_screen_header(
    frame: &mut Frame<'_>,
    header_area: Rect,
    derived_summary: &DerivedSummary,
) {
    let header_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(12), Constraint::Length(30)])
        .split(header_area);

    frame.render_widget(
        Paragraph::new(format!("Updated {}", Local::now().format("%-I:%M%P"))).style(muted_style()),
        header_layout[0],
    );
    frame.render_widget(
        Paragraph::new(data_quality_summary_label(
            derived_summary.data_quality_notices.len(),
        ))
        .alignment(Alignment::Right)
        .style(muted_style()),
        header_layout[1],
    );
}

/// Builds the compact Headline Period table from cost-first summary data.
fn summary_table(
    derived_summary: &DerivedSummary,
    terminal_interface_state: &TerminalInterfaceState,
) -> Table<'static> {
    let rows = derived_summary
        .headline_periods
        .iter()
        .enumerate()
        .map(|(headline_period_index, headline_period)| {
            let row_style = if headline_period_index
                == terminal_interface_state.selected_headline_period_index
            {
                selected_row_style()
            } else {
                matte_box_style()
            };

            Row::new(vec![
                Cell::from(reporting_period_label(&headline_period.kind)),
                Cell::from(format_united_states_dollar_cost(
                    headline_period
                        .summary_totals
                        .known_united_states_dollar_cost,
                )),
                Cell::from(format!(
                    "{} tokens",
                    compact_token_count(headline_period.summary_totals.token_totals.total_tokens)
                )),
                Cell::from(format!(
                    "{} sessions",
                    headline_period.summary_totals.session_count
                )),
                Cell::from(period_cost_change_label(headline_period))
                    .style(period_cost_change_style(headline_period)),
            ])
            .style(row_style)
        })
        .collect::<Vec<_>>();

    Table::new(
        rows,
        [
            Constraint::Length(13),
            Constraint::Length(10),
            Constraint::Length(18),
            Constraint::Length(14),
            Constraint::Length(22),
        ],
    )
    .style(matte_box_style())
}

/// Builds the Session Detail table.
fn session_panel(
    derived_summary: &DerivedSummary,
    terminal_interface_state: &TerminalInterfaceState,
    session_area: Rect,
) -> Table<'static> {
    let maximum_visible_session_count = session_area.height.saturating_sub(4).max(1) as usize;
    Table::new(
        session_table_rows(
            derived_summary,
            terminal_interface_state,
            maximum_visible_session_count,
        ),
        session_table_column_constraints(session_area.width),
    )
    .column_spacing(SESSION_TABLE_COLUMN_SPACING)
    .header(
        Row::new(vec![
            Cell::from("Status"),
            Cell::from("Time"),
            Cell::from("Project"),
            Cell::from("Agent"),
            Cell::from("Model"),
            Cell::from("Cost"),
            Cell::from("Price / Million"),
            Cell::from("Tokens"),
        ])
        .style(header_style())
        .bottom_margin(1),
    )
    .block(
        Block::default()
            .title(Line::from(" Sessions ").style(title_style()))
            .borders(Borders::TOP)
            .border_style(border_style()),
    )
    .style(matte_box_style())
}

/// Builds Session Detail table column widths that consume the full available panel width.
fn session_table_column_constraints(session_area_width: u16) -> [Constraint; 8] {
    let session_table_column_gap_count =
        SESSION_TABLE_MINIMUM_COLUMN_WIDTHS.len().saturating_sub(1) as u16;
    let spacing_width = SESSION_TABLE_COLUMN_SPACING * session_table_column_gap_count;
    let available_column_width = session_area_width.saturating_sub(spacing_width);
    let minimum_column_width: u16 = SESSION_TABLE_MINIMUM_COLUMN_WIDTHS.iter().copied().sum();

    if available_column_width <= minimum_column_width {
        return SESSION_TABLE_MINIMUM_COLUMN_WIDTHS.map(Constraint::Length);
    }

    let column_count = SESSION_TABLE_MINIMUM_COLUMN_WIDTHS.len() as u16;
    let extra_width = available_column_width - minimum_column_width;
    let shared_extra_width = extra_width / column_count;
    let remaining_extra_width = extra_width % column_count;

    let mut column_widths = SESSION_TABLE_MINIMUM_COLUMN_WIDTHS;
    for (column_index, column_width) in column_widths.iter_mut().enumerate() {
        *column_width += shared_extra_width;
        if (column_index as u16) < remaining_extra_width {
            *column_width += 1;
        }
    }

    column_widths.map(Constraint::Length)
}

/// Builds table rows for the currently selected Reporting Period.
fn session_table_rows(
    derived_summary: &DerivedSummary,
    terminal_interface_state: &TerminalInterfaceState,
    maximum_visible_session_count: usize,
) -> Vec<Row<'static>> {
    let sessions = terminal_interface_state
        .selected_period(derived_summary)
        .map(|period| period.session_detail.as_slice())
        .unwrap_or(&[]);

    if sessions.is_empty() {
        return vec![Row::new(vec![
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from("No sessions for selected period").style(muted_style()),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ])];
    }

    let visible_start_index = visible_session_start_index(
        terminal_interface_state.selected_session_index,
        sessions.len(),
        maximum_visible_session_count,
    );

    sessions
        .iter()
        .enumerate()
        .skip(visible_start_index)
        .take(maximum_visible_session_count)
        .map(|(session_index, priced_session)| {
            let project_name = priced_session
                .codex_session
                .project_name
                .as_deref()
                .unwrap_or("(no project)");
            let row_style = if session_index == terminal_interface_state.selected_session_index {
                selected_row_style()
            } else {
                matte_box_style()
            };

            Row::new(vec![
                Cell::from(session_status_label(priced_session))
                    .style(session_status_style(priced_session)),
                Cell::from(session_time_label(priced_session)),
                Cell::from(project_name.to_owned()),
                Cell::from(priced_session.codex_session.ai_coding_agent.label()),
                Cell::from(priced_session.codex_session.compact_model_label()),
                Cell::from(session_cost_label(&priced_session.cost_state)),
                Cell::from(session_pricing_label(
                    priced_session.price_schedule_match.as_ref(),
                )),
                Cell::from(format!(
                    "{} tokens",
                    compact_token_count(priced_session.codex_session.token_totals.total_tokens)
                )),
            ])
            .style(row_style)
        })
        .collect()
}

fn visible_session_start_index(
    selected_session_index: usize,
    session_count: usize,
    maximum_visible_session_count: usize,
) -> usize {
    if maximum_visible_session_count == 0 || session_count <= maximum_visible_session_count {
        return 0;
    }

    selected_session_index
        .saturating_add(1)
        .saturating_sub(maximum_visible_session_count)
        .min(session_count - maximum_visible_session_count)
}

fn matte_box_style() -> Style {
    Style::default()
        .bg(MATTE_BOX_BACKGROUND)
        .fg(MATTE_BOX_FOREGROUND)
}

fn title_style() -> Style {
    matte_box_style()
        .fg(MATTE_BOX_FOREGROUND)
        .add_modifier(Modifier::BOLD)
}

fn header_style() -> Style {
    matte_box_style()
        .fg(MATTE_BOX_MUTED_FOREGROUND)
        .add_modifier(Modifier::BOLD)
}

fn muted_style() -> Style {
    matte_box_style().fg(MATTE_BOX_MUTED_FOREGROUND)
}

fn border_style() -> Style {
    matte_box_style().fg(MATTE_BOX_MUTED_FOREGROUND)
}

fn selected_row_style() -> Style {
    matte_box_style().bg(MATTE_BOX_SELECTED_BACKGROUND)
}

fn footer_paragraph() -> Paragraph<'static> {
    Paragraph::new(Line::from(vec![
        Span::raw(" ↑/↓ move   "),
        Span::raw("←/→ period   "),
        Span::raw("r reload   "),
        Span::raw("q quit"),
    ]))
    .style(matte_box_style())
    .alignment(Alignment::Center)
}

fn horizontal_separator(width: u16) -> Paragraph<'static> {
    Paragraph::new("─".repeat(width as usize)).style(border_style())
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

fn render_headline_period_row(headline_period: &HeadlinePeriod) -> String {
    format!(
        "{:<11} {:>8}  {:>12} tokens  {:>3} sessions  {}",
        reporting_period_label(&headline_period.kind),
        format_united_states_dollar_cost(
            headline_period
                .summary_totals
                .known_united_states_dollar_cost
        ),
        compact_token_count(headline_period.summary_totals.token_totals.total_tokens),
        headline_period.summary_totals.session_count,
        period_cost_change_label(headline_period)
    )
}

fn compact_session_row(priced_session: &PricedCodexSession, project_name: &str) -> String {
    format!(
        "{:<6}  {:<7}  {:<10}  {:<6}  {:<14}  {:>7}  {:<25}  {:>14}",
        session_status_label(priced_session),
        session_time_label(priced_session),
        project_name,
        priced_session.codex_session.ai_coding_agent.label(),
        priced_session.codex_session.compact_model_label(),
        session_cost_label(&priced_session.cost_state),
        session_pricing_label(priced_session.price_schedule_match.as_ref()),
        format!(
            "{} tokens",
            compact_token_count(priced_session.codex_session.token_totals.total_tokens)
        )
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

fn period_cost_change_label(headline_period: &HeadlinePeriod) -> String {
    let Some(previous_period_known_united_states_dollar_cost) =
        headline_period.previous_period_known_united_states_dollar_cost
    else {
        return "no comparison".to_owned();
    };

    let known_united_states_dollar_cost_change = headline_period
        .summary_totals
        .known_united_states_dollar_cost
        - previous_period_known_united_states_dollar_cost;

    format!(
        "{} {}",
        format_signed_united_states_dollar_cost(known_united_states_dollar_cost_change),
        previous_period_comparison_label(&headline_period.kind)
    )
}

fn period_cost_change_style(headline_period: &HeadlinePeriod) -> Style {
    let Some(previous_period_known_united_states_dollar_cost) =
        headline_period.previous_period_known_united_states_dollar_cost
    else {
        return muted_style();
    };

    let known_united_states_dollar_cost_change = headline_period
        .summary_totals
        .known_united_states_dollar_cost
        - previous_period_known_united_states_dollar_cost;

    if known_united_states_dollar_cost_change > rust_decimal::Decimal::ZERO {
        return matte_box_style().fg(MATTE_BOX_WARNING);
    }
    if known_united_states_dollar_cost_change < rust_decimal::Decimal::ZERO {
        return matte_box_style().fg(MATTE_BOX_SUCCESS);
    }
    muted_style()
}

fn format_signed_united_states_dollar_cost(cost: rust_decimal::Decimal) -> String {
    let absolute_cost = if cost < rust_decimal::Decimal::ZERO {
        -cost
    } else {
        cost
    };
    let formatted_cost = format_united_states_dollar_cost(absolute_cost);

    if cost > rust_decimal::Decimal::ZERO {
        format!("+{formatted_cost}")
    } else if cost < rust_decimal::Decimal::ZERO {
        format!("-{formatted_cost}")
    } else {
        formatted_cost
    }
}

fn previous_period_comparison_label(kind: &ReportingPeriodKind) -> &'static str {
    match kind {
        ReportingPeriodKind::Daily => "vs yesterday",
        ReportingPeriodKind::Weekly => "vs last week",
        ReportingPeriodKind::Monthly => "vs last month",
        ReportingPeriodKind::AllTime => "vs previous all time",
    }
}

/// Formats token counts with compact suffixes for table columns.
fn compact_token_count(token_count: Option<u64>) -> String {
    let Some(token_count) = token_count else {
        return "unknown".to_owned();
    };

    if token_count >= 1_000_000 {
        return format!("{:.2}M", token_count as f64 / 1_000_000.0);
    }
    if token_count >= 1_000 {
        return format!("{:.1}K", token_count as f64 / 1_000.0);
    }
    token_count.to_string()
}

fn session_status_label(priced_session: &PricedCodexSession) -> &'static str {
    if priced_session.codex_session.is_active {
        return "● Act";
    }
    match priced_session.cost_state {
        CostState::Complete { .. } => "● OK",
        CostState::Partial { .. } | CostState::Incomplete { .. } => "◐ Inc",
    }
}

fn session_status_style(priced_session: &PricedCodexSession) -> Style {
    if priced_session.codex_session.is_active {
        return matte_box_style().fg(MATTE_BOX_ACCENT);
    }
    match priced_session.cost_state {
        CostState::Complete { .. } => matte_box_style().fg(MATTE_BOX_SUCCESS),
        CostState::Partial { .. } | CostState::Incomplete { .. } => {
            matte_box_style().fg(MATTE_BOX_WARNING)
        }
    }
}

fn session_time_label(priced_session: &PricedCodexSession) -> String {
    priced_session
        .codex_session
        .session_detail_time()
        .with_timezone(&Local)
        .format("%-I:%M%P")
        .to_string()
}

fn session_pricing_label(price_schedule_match: Option<&PriceScheduleMatch>) -> String {
    let Some(price_schedule_match) = price_schedule_match else {
        return "Unpriced".to_owned();
    };

    format!(
        "Input {} Output {}",
        format_united_states_dollar_price_per_million_tokens(
            price_schedule_match.input_tokens_per_million
        ),
        format_united_states_dollar_price_per_million_tokens(
            price_schedule_match.output_tokens_per_million
        )
    )
}

fn format_united_states_dollar_price_per_million_tokens(
    price_per_million_tokens: Decimal,
) -> String {
    let normalized_price = price_per_million_tokens.normalize();
    if normalized_price.scale() > 2 {
        format!("${normalized_price}")
    } else {
        format!("${normalized_price:.2}")
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
        } => format_united_states_dollar_cost(*known_united_states_dollar_cost),
        CostState::Incomplete { .. } => "—".to_owned(),
    }
}

fn reporting_period_label(kind: &ReportingPeriodKind) -> &'static str {
    match kind {
        ReportingPeriodKind::Daily => "Today",
        ReportingPeriodKind::Weekly => "This Week",
        ReportingPeriodKind::Monthly => "This Month",
        ReportingPeriodKind::AllTime => "All Time",
    }
}

fn data_quality_summary_label(data_quality_notice_count: usize) -> String {
    if data_quality_notice_count == 0 {
        return "Data Quality: OK".to_owned();
    }

    let notice_word = if data_quality_notice_count == 1 {
        "notice"
    } else {
        "notices"
    };
    format!("Data Quality: {data_quality_notice_count} {notice_word}")
}

fn session_table_header() -> String {
    format!(
        "{:<6}  {:<7}  {:<10}  {:<6}  {:<14}  {:>7}  {:<25}  {:>14}",
        "Status", "Time", "Project", "Agent", "Model", "Cost", "Price / Million", "Tokens"
    )
}

fn session_table_separator() -> String {
    format!(
        "{:<6}  {:<7}  {:<10}  {:<6}  {:<14}  {:>7}  {:<25}  {:>14}",
        "──────",
        "───────",
        "──────────",
        "──────",
        "──────────────",
        "───────",
        "─────────────────────────",
        "──────────────"
    )
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;

    use super::*;
    use crate::cost::{IncompleteCostReason, PriceScheduleMatch};
    use crate::reporting_period::{
        HeadlinePeriod, PeriodCostState, SummaryTotals, build_derived_summary_at,
    };
    use crate::session::{
        AiCodingAgent, CodexSession, DataQualityNotice, DataQualityNoticeKind, TokenTotals,
    };
    use crate::usage_source::CurrentSourceState;

    #[test]
    fn terminal_summary_renders_headline_tokens_cost_changes_and_data_quality() {
        let derived_summary = DerivedSummary {
            current_source_state: CurrentSourceState::Readable {
                path: "source".into(),
                is_custom: false,
            },
            headline_periods: vec![
                headline_period(
                    ReportingPeriodKind::Daily,
                    PeriodCostState::ZeroUsage,
                    Decimal::ZERO,
                    Some(Decimal::from(1)),
                    Vec::new(),
                ),
                headline_period(
                    ReportingPeriodKind::Weekly,
                    PeriodCostState::Partial {
                        reasons: vec![IncompleteCostReason::UnpricedUsage {
                            model: "unknown".to_owned(),
                        }],
                    },
                    Decimal::from(3),
                    Some(Decimal::from(2)),
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
                    Decimal::from(1),
                    Some(Decimal::from(3)),
                    Vec::new(),
                ),
                headline_period(
                    ReportingPeriodKind::AllTime,
                    PeriodCostState::Complete,
                    Decimal::from(5),
                    None,
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

        assert!(rendered_summary.contains("Today"));
        assert!(rendered_summary.contains("15 tokens"));
        assert!(rendered_summary.contains("This Week"));
        assert!(rendered_summary.contains("This Month"));
        assert!(rendered_summary.contains("All Time"));
        assert!(rendered_summary.contains("+$1.00 vs last week"));
        assert!(rendered_summary.contains("-$2.00 vs last month"));
        assert!(rendered_summary.contains("no comparison"));
        assert!(!rendered_summary.contains("▇"));
        assert!(rendered_summary.contains("Data Quality: 1 notice"));
        assert!(rendered_summary.contains("● Act"));
        assert!(rendered_summary.contains("Session"));
        assert!(rendered_summary.contains("Price / Million"));
        assert!(rendered_summary.contains("Input $5.00 Output $30.00"));
    }

    #[test]
    fn session_detail_and_all_time_detail_render_newest_first() {
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
            CurrentSourceState::Readable {
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

        assert!(session_detail[0].contains("newer"));
        assert!(session_detail[0].contains("gpt-5.5"));
        assert!(session_detail[0].contains("Input $5.00 Output $30.00"));
        assert!(session_detail[0].contains("15 tokens"));
        assert_eq!(
            derived_summary.all_time_detail[0].session_detail[0],
            newer_session
        );
    }

    #[test]
    fn session_table_columns_use_full_available_width() {
        let constraints = session_table_column_constraints(140);
        let column_widths = constraints
            .iter()
            .map(session_table_constraint_width)
            .collect::<Vec<_>>();
        let total_column_width = column_widths.iter().sum::<u16>();
        let spacing_width =
            SESSION_TABLE_COLUMN_SPACING * (constraints.len().saturating_sub(1) as u16);

        assert_eq!(total_column_width + spacing_width, 140);
        assert_eq!(column_widths, vec![12, 13, 16, 12, 20, 13, 30, 17]);
    }

    #[test]
    fn session_table_has_ai_coding_agent_column_header() {
        let derived_summary = build_derived_summary_at(
            CurrentSourceState::Readable {
                path: "source".into(),
                is_custom: false,
            },
            vec![priced_session(
                "agent-header",
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
        let terminal_interface_state = TerminalInterfaceState::new(&derived_summary);
        let terminal_backend = ratatui::backend::TestBackend::new(120, 8);
        let mut terminal = Terminal::new(terminal_backend).expect("terminal");

        terminal
            .draw(|frame| {
                let session_area = Rect::new(0, 0, 120, 8);
                frame.render_widget(
                    session_panel(&derived_summary, &terminal_interface_state, session_area),
                    session_area,
                );
            })
            .expect("draw session table");

        let terminal_output = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        let status_header_index = terminal_output.find("Status").expect("status header");
        let time_header_index = terminal_output.find("Time").expect("time header");
        let project_header_index = terminal_output.find("Project").expect("project header");
        let agent_header_index = terminal_output.find("Agent").expect("agent header");
        let model_header_index = terminal_output.find("Model").expect("model header");
        let cost_header_index = terminal_output.find("Cost").expect("cost header");
        let price_header_index = terminal_output
            .find("Price / Million")
            .expect("price header");
        let tokens_header_index = terminal_output.find("Tokens").expect("tokens header");

        assert!(status_header_index < time_header_index);
        assert!(time_header_index < project_header_index);
        assert!(project_header_index < agent_header_index);
        assert!(agent_header_index < model_header_index);
        assert!(model_header_index < cost_header_index);
        assert!(cost_header_index < price_header_index);
        assert!(price_header_index < tokens_header_index);
    }

    #[test]
    fn session_table_renders_full_price_label_at_standard_terminal_width() {
        let derived_summary = build_derived_summary_at(
            CurrentSourceState::Readable {
                path: "source".into(),
                is_custom: false,
            },
            vec![priced_session(
                "standard-width",
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
        let terminal_interface_state = TerminalInterfaceState::new(&derived_summary);
        let terminal_backend = ratatui::backend::TestBackend::new(120, 8);
        let mut terminal = Terminal::new(terminal_backend).expect("terminal");

        terminal
            .draw(|frame| {
                let session_area = Rect::new(0, 0, 120, 8);
                frame.render_widget(
                    session_panel(&derived_summary, &terminal_interface_state, session_area),
                    session_area,
                );
            })
            .expect("draw session table");

        let terminal_output = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(terminal_output.contains("Input $5.00 Output $30.00"));
    }

    #[test]
    fn session_time_label_prefers_last_modified_time() {
        let mut priced_session = priced_session(
            "modified-time",
            false,
            CostState::Complete {
                united_states_dollar_cost: Decimal::from(1),
            },
        );
        let session_start_time = Utc.with_ymd_and_hms(2026, 5, 10, 10, 0, 0).unwrap();
        let session_last_modified_time = Utc.with_ymd_and_hms(2026, 5, 10, 11, 30, 0).unwrap();
        priced_session.codex_session.session_start_time = session_start_time;
        priced_session.codex_session.session_last_modified_time = Some(session_last_modified_time);

        let start_time_label = session_start_time
            .with_timezone(&Local)
            .format("%-I:%M%P")
            .to_string();
        let last_modified_time_label = session_last_modified_time
            .with_timezone(&Local)
            .format("%-I:%M%P")
            .to_string();

        assert_eq!(
            session_time_label(&priced_session),
            last_modified_time_label
        );
        assert_ne!(session_time_label(&priced_session), start_time_label);
    }

    #[test]
    fn keyboard_navigation_supports_arrows_and_terminal_native_keys() {
        let derived_summary = build_derived_summary_at(
            CurrentSourceState::Readable {
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
    fn reload_and_quit_keys_have_terminal_actions() {
        let derived_summary = build_derived_summary_at(
            CurrentSourceState::Readable {
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
            CurrentSourceState::Readable {
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
            terminal_interface_state.handle_key_code(KeyCode::Char('r'), &derived_summary),
            TerminalAction::Reload
        );

        terminal_interface_state.reconcile_with_summary(&reloaded_summary);

        assert_eq!(terminal_interface_state.selected_session_index, 0);
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
        known_united_states_dollar_cost: Decimal,
        previous_period_known_united_states_dollar_cost: Option<Decimal>,
        session_detail: Vec<PricedCodexSession>,
    ) -> HeadlinePeriod {
        HeadlinePeriod {
            kind,
            local_start_date: None,
            local_end_date: None,
            summary_totals: SummaryTotals {
                known_united_states_dollar_cost,
                period_cost_state,
                token_totals: TokenTotals {
                    input_tokens: Some(10),
                    output_tokens: Some(5),
                    total_tokens: Some(15),
                    ..TokenTotals::default()
                },
                session_count: session_detail.len(),
            },
            previous_period_known_united_states_dollar_cost,
            session_detail,
        }
    }

    fn session_table_constraint_width(constraint: &Constraint) -> u16 {
        match constraint {
            Constraint::Length(width) => *width,
            _ => panic!("session table column constraints should be concrete lengths"),
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
                ai_coding_agent: AiCodingAgent::Codex,
                source_path: format!("{project_name}.jsonl").into(),
                session_name: Some(format!("{project_name} session")),
                session_start_time: Utc.with_ymd_and_hms(2026, 5, 10, 10, minute, 0).unwrap(),
                session_last_modified_time: None,
                model: "gpt-5.5".to_owned(),
                reasoning_effort: None,
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
                input_tokens_per_million: Decimal::from_str_exact("5.000").unwrap(),
                cache_read_tokens_per_million: Decimal::from_str_exact("0.500").unwrap(),
                cache_write_tokens_per_million: Decimal::from_str_exact("5.000").unwrap(),
                output_tokens_per_million: Decimal::from_str_exact("30.000").unwrap(),
                reasoning_output_tokens_per_million: None,
            }),
        }
    }
}
