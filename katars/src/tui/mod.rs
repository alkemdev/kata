use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{io, time::Duration};
use tracing::{debug, info};

use crate::ks;

// ── REPL state ────────────────────────────────────────────────────────────────

/// A single entry in the REPL history.
#[derive(Debug)]
struct HistoryEntry {
    input: String,
    /// `Ok(output)` for successful evaluation, `Err(msg)` for errors.
    result: Result<String, String>,
}

struct ReplState {
    history: Vec<HistoryEntry>,
    input: String,
    /// Number of lines scrolled up from the bottom.
    scroll: u16,
    /// Persistent interpreter — keeps state across submissions.
    interp: ks::Interpreter,
}

impl ReplState {
    fn new() -> Self {
        let mut interp = ks::Interpreter::new();

        // Load the prelude so std types (Opt, Res, Arr, etc.) are available.
        let prelude_src = include_str!("../../../std/prelude.ks");
        if let Ok(prelude) = ks::parse(prelude_src, "<prelude>") {
            let mut sink = Vec::new();
            let _ = interp.exec_program(&prelude, None, &mut sink);
        }

        Self {
            history: Vec::new(),
            input: String::new(),
            scroll: 0,
            interp,
        }
    }

    fn submit(&mut self) {
        let input = std::mem::take(&mut self.input).trim().to_string();
        if input.is_empty() {
            return;
        }

        info!(input = %input, "repl submit");

        let source = if input.ends_with(';') {
            input.clone()
        } else {
            format!("{input};")
        };

        let result = match ks::parse(&source, "<repl>") {
            Err(()) => Err("parse error".to_string()),
            Ok(program) => {
                let mut buf = Vec::new();
                match self.interp.exec_repl(&program, &mut buf) {
                    Ok(()) => Ok(String::from_utf8_lossy(&buf).into_owned()),
                    Err(e) => {
                        let msg = e.kind.format_with(self.interp.type_registry());
                        Err(msg)
                    }
                }
            }
        };

        debug!(?result, "repl result");
        self.history.push(HistoryEntry { input, result });
        self.scroll = 0;
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn render(f: &mut Frame, state: &ReplState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // history
            Constraint::Length(3), // input line
        ])
        .split(f.area());

    // ── History pane ─────────────────────────────────────────────────────────

    let mut lines: Vec<Line> = Vec::new();
    for entry in &state.history {
        // Input line.
        lines.push(Line::from(vec![
            Span::styled(
                "❯ ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(entry.input.clone()),
        ]));
        // Output / error line.
        match &entry.result {
            Ok(out) if out.is_empty() => {}
            Ok(out) => {
                for line in out.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {line}"),
                        Style::default().fg(Color::White),
                    )));
                }
            }
            Err(e) => {
                lines.push(Line::from(Span::styled(
                    format!("  error: {e}"),
                    Style::default().fg(Color::Red),
                )));
            }
        }
        lines.push(Line::from(""));
    }

    // Scroll: show the bottom of history by default.
    let history_height = chunks[0].height.saturating_sub(2) as usize; // minus borders
    let total_lines = lines.len();
    let scroll_offset = if total_lines > history_height {
        (total_lines - history_height).saturating_sub(state.scroll as usize) as u16
    } else {
        0
    };

    let history_widget = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(" KataScript REPL ")
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    f.render_widget(history_widget, chunks[0]);

    // ── Input line ────────────────────────────────────────────────────────────

    let input_text = format!("❯ {}_", state.input);
    let input_widget = Paragraph::new(input_text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Input  (Enter to run · Esc or Ctrl+C to quit) ")
                .title_style(Style::default().fg(Color::DarkGray)),
        );

    f.render_widget(input_widget, chunks[1]);
}

// ── Event loop ────────────────────────────────────────────────────────────────

pub fn run_repl() -> io::Result<()> {
    info!("starting REPL");

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = ReplState::new();
    let mut running = true;

    while running {
        terminal.draw(|f| render(f, &state))?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Enter => state.submit(),

                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        running = false;
                    }

                    KeyCode::Esc => running = false,

                    KeyCode::Char(c) => state.input.push(c),

                    KeyCode::Backspace => {
                        state.input.pop();
                    }

                    KeyCode::Up => state.scroll = state.scroll.saturating_add(1),
                    KeyCode::Down => state.scroll = state.scroll.saturating_sub(1),

                    _ => {}
                }
            }
        }
    }

    // Restore terminal.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    info!("REPL exited");
    Ok(())
}
