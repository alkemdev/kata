use std::borrow::Cow;
use std::io;
use std::sync::{Arc, Mutex};

use owo_colors::OwoColorize;
use reedline::{
    default_emacs_keybindings, ColumnarMenu, Emacs, FileBackedHistory, KeyCode, KeyModifiers,
    Prompt, PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus, Reedline,
    ReedlineEvent, ReedlineMenu, Signal, Span as RlSpan, Suggestion, ValidationResult,
};
use tracing::info;

use crate::ks;
use crate::ks::lexer::KEYWORDS;

// ── Completer ────────────────────────────────────────────────────────────────

struct KataCompleter {
    interp: Arc<Mutex<ks::Interpreter>>,
}

impl reedline::Completer for KataCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let start = line[..pos]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
            .map_or(0, |i| i + 1);
        let token = &line[start..pos];
        if token.is_empty() {
            return Vec::new();
        }

        let interp = self.interp.lock().unwrap();

        // Dot-path completion: "std.mem." → entries of mem module.
        if let Some(dot_pos) = token.rfind('.') {
            let receiver = &token[..dot_pos];
            let attr_prefix = &token[dot_pos + 1..];
            let segments: Vec<&str> = receiver.split('.').collect();
            let attrs = interp.completions_for_path(&segments);
            return attrs
                .into_iter()
                .filter(|name| name.starts_with(attr_prefix))
                .map(|name| Suggestion {
                    value: name.clone(),
                    display_override: None,
                    description: None,
                    style: None,
                    extra: None,
                    span: RlSpan::new(start + dot_pos + 1, pos),
                    append_whitespace: false,
                    match_indices: None,
                })
                .collect();
        }

        // Simple name: keywords + scope names.
        let scope_names = interp.visible_names();
        let mut candidates: Vec<String> = KEYWORDS
            .iter()
            .map(|s| s.to_string())
            .chain(scope_names)
            .filter(|name| name.starts_with(token))
            .collect();
        candidates.sort();
        candidates.dedup();

        candidates
            .into_iter()
            .map(|name| Suggestion {
                value: name,
                display_override: None,
                description: None,
                style: None,
                extra: None,
                span: RlSpan::new(start, pos),
                append_whitespace: false,
                match_indices: None,
            })
            .collect()
    }
}

// ── Validator (multi-line) ───────────────────────────────────────────────────

struct KataValidator;

impl reedline::Validator for KataValidator {
    fn validate(&self, line: &str) -> ValidationResult {
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escape = false;
        for ch in line.chars() {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' && in_string {
                escape = true;
                continue;
            }
            if ch == '"' || ch == '\'' {
                in_string = !in_string;
                continue;
            }
            if in_string {
                continue;
            }
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
        }
        if depth > 0 {
            ValidationResult::Incomplete
        } else {
            ValidationResult::Complete
        }
    }
}

// ── Prompt ───────────────────────────────────────────────────────────────────

struct KataPrompt;

impl Prompt for KataPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        Cow::Owned("λ ".cyan().bold().to_string())
    }

    fn render_prompt_right(&self) -> Cow<str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _mode: PromptEditMode) -> Cow<str> {
        Cow::Borrowed("")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        Cow::Owned("· ".dimmed().to_string())
    }

    fn render_prompt_history_search_indicator(&self, search: PromptHistorySearch) -> Cow<str> {
        let prefix = match search.status {
            PromptHistorySearchStatus::Passing => "search",
            PromptHistorySearchStatus::Failing => "search (not found)",
        };
        Cow::Owned(format!("{}: ", prefix.dimmed()))
    }
}

// ── Highlighter ──────────────────────────────────────────────────────────────

struct KataHighlighter;

impl reedline::Highlighter for KataHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> reedline::StyledText {
        use nu_ansi_term::{Color, Style};

        let mut styled = reedline::StyledText::new();
        // Simple token-level highlighting.
        let tokens = ks::lexer::lex(line);
        let mut last_end = 0;

        for tok in &tokens {
            // Emit any gap (whitespace) between tokens.
            if tok.start > last_end {
                styled.push((Style::default(), line[last_end..tok.start].to_string()));
            }
            let text = &line[tok.start..tok.end];
            let style = match &tok.token {
                // Keywords
                ks::lexer::Token::True
                | ks::lexer::Token::False
                | ks::lexer::Token::Nil
                | ks::lexer::Token::Let
                | ks::lexer::Token::Func
                | ks::lexer::Token::If
                | ks::lexer::Token::Else
                | ks::lexer::Token::Elif
                | ks::lexer::Token::Enum
                | ks::lexer::Token::While
                | ks::lexer::Token::For
                | ks::lexer::Token::In
                | ks::lexer::Token::With
                | ks::lexer::Token::Kind
                | ks::lexer::Token::Impl
                | ks::lexer::Token::Type
                | ks::lexer::Token::As
                | ks::lexer::Token::Bail
                | ks::lexer::Token::Cont
                | ks::lexer::Token::Ret
                | ks::lexer::Token::Unsafe
                | ks::lexer::Token::Import
                | ks::lexer::Token::Match
                | ks::lexer::Token::SelfValue
                | ks::lexer::Token::SelfType => Color::Magenta.bold(),
                // Strings
                ks::lexer::Token::Str(_) => Color::Green.normal(),
                // Numbers
                ks::lexer::Token::Num(_) => Color::Cyan.normal(),
                // Identifiers — check if type name (starts with uppercase)
                ks::lexer::Token::Ident(name) => {
                    if name.chars().next().map_or(false, |c| c.is_uppercase()) {
                        Color::Yellow.normal()
                    } else {
                        Style::default()
                    }
                }
                // Everything else (operators, punctuation)
                _ => Style::default(),
            };
            styled.push((style, text.to_string()));
            last_end = tok.end;
        }
        // Trailing text after last token.
        if last_end < line.len() {
            styled.push((Style::default(), line[last_end..].to_string()));
        }
        styled
    }
}

// ── Input execution ──────────────────────────────────────────────────────────

fn execute(interp: &mut ks::Interpreter, input: &str) {
    let source = if input.trim_end().ends_with(';') {
        input.to_string()
    } else {
        format!("{input};")
    };

    match ks::parse(&source, "<repl>") {
        Err(()) => eprintln!("{} parse error", "error:".red()),
        Ok(program) => {
            let mut buf = Vec::new();
            match interp.exec_repl(&program, &mut buf) {
                Ok(()) => {
                    let out = String::from_utf8_lossy(&buf);
                    if !out.is_empty() {
                        print!("{out}");
                    }
                }
                Err(e) => {
                    ks::render_error(&e, interp.type_registry(), &source, "<repl>");
                }
            }
        }
    }
}

// ── Paths ────────────────────────────────────────────────────────────────────

fn data_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("kata")
}

fn history_path() -> std::path::PathBuf {
    data_dir().join("history")
}

// ── REPL entry point ─────────────────────────────────────────────────────────

pub fn run_repl() -> io::Result<()> {
    info!("starting REPL");

    // Persistent interpreter with prelude.
    let interp = Arc::new(Mutex::new(ks::Interpreter::new()));
    {
        let prelude_src = include_str!("../../../std/prelude.ks");
        if let Ok(prelude) = ks::parse(prelude_src, "<prelude>") {
            let mut sink = Vec::new();
            let _ = interp
                .lock()
                .unwrap()
                .exec_program(&prelude, None, &mut sink);
        }
    }

    // Ensure data directory exists.
    let hist_path = history_path();
    if let Some(parent) = hist_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let history = Box::new(
        FileBackedHistory::with_file(1000, hist_path)
            .map_err(|e| io::Error::other(e.to_string()))?,
    );

    let completer = Box::new(KataCompleter {
        interp: Arc::clone(&interp),
    });

    // Default ColumnarMenu has name "columnar_menu".
    let completion_menu = Box::new(ColumnarMenu::default());

    // Tab triggers completion menu; repeated Tab cycles through items.
    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("columnar_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    let edit_mode = Box::new(Emacs::new(keybindings));

    let mut line_editor = Reedline::create()
        .with_completer(completer)
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_history(history)
        .with_validator(Box::new(KataValidator))
        .with_highlighter(Box::new(KataHighlighter))
        .with_edit_mode(edit_mode);

    println!("{} {}", "kata".cyan().bold(), "(Ctrl+D to exit)".dimmed());

    let prompt = KataPrompt;

    loop {
        match line_editor.read_line(&prompt) {
            Ok(Signal::Success(input)) => {
                let input = input.trim();
                if !input.is_empty() {
                    execute(&mut interp.lock().unwrap(), input);
                }
            }
            Ok(Signal::CtrlC) => {
                // Cancel current line, continue.
            }
            Ok(Signal::CtrlD) => {
                break;
            }
            Err(e) => {
                eprintln!("{} {e}", "error:".red());
                break;
            }
        }
    }

    info!("REPL exited");
    Ok(())
}
