use std::borrow::Cow;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use owo_colors::OwoColorize;
use rustyline::completion::{Completer, Pair};
use rustyline::config::CompletionType;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Config, EditMode, Editor, Helper};
use tracing::info;

use crate::ks;
use crate::ks::lexer::KEYWORDS;

// ── Completion ───────────────────────────────────────────────────────────────

struct KataHelper {
    interp: Rc<RefCell<ks::Interpreter>>,
}

impl Completer for KataHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        let start = line[..pos]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
            .map_or(0, |i| i + 1);
        let token = &line[start..pos];
        if token.is_empty() {
            return Ok((start, Vec::new()));
        }

        let interp = self.interp.borrow();

        // Dot-path completion: "std." or "obj.field."
        if let Some(dot_pos) = token.rfind('.') {
            let receiver = &token[..dot_pos];
            let attr_prefix = &token[dot_pos + 1..];
            let segments: Vec<&str> = receiver.split('.').collect();
            let attrs = interp.completions_for_path(&segments);
            let matches: Vec<Pair> = attrs
                .into_iter()
                .filter(|name| name.starts_with(attr_prefix))
                .map(|name| Pair {
                    display: name.clone(),
                    replacement: name,
                })
                .collect();
            return Ok((start + dot_pos + 1, matches));
        }

        // Simple name: keywords + scope names.
        let scope_names = interp.visible_names();
        let mut matches: Vec<Pair> = KEYWORDS
            .iter()
            .map(|s| s.to_string())
            .chain(scope_names)
            .filter(|name| name.starts_with(token))
            .map(|name| Pair {
                display: name.clone(),
                replacement: name,
            })
            .collect();
        matches.sort_by(|a, b| a.display.cmp(&b.display));
        matches.dedup_by(|a, b| a.display == b.display);
        Ok((start, matches))
    }
}

impl Hinter for KataHelper {
    type Hint = String;
}

impl Highlighter for KataHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        if prompt.starts_with('·') {
            Cow::Owned(prompt.dimmed().to_string())
        } else {
            Cow::Owned(prompt.cyan().bold().to_string())
        }
    }
}

impl Validator for KataHelper {}
impl Helper for KataHelper {}

// ── Multi-line detection ─────────────────────────────────────────────────────

fn is_balanced(input: &str) -> bool {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for ch in input.chars() {
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
    depth <= 0
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
    let interp = Rc::new(RefCell::new(ks::Interpreter::new()));
    {
        let prelude_src = include_str!("../../../std/prelude.ks");
        if let Ok(prelude) = ks::parse(prelude_src, "<prelude>") {
            let mut sink = Vec::new();
            let _ = interp.borrow_mut().exec_program(&prelude, None, &mut sink);
        }
    }

    let config = Config::builder()
        .edit_mode(EditMode::Emacs)
        .completion_type(CompletionType::List)
        .build();
    let mut rl = Editor::with_config(config).map_err(|e| io::Error::other(e.to_string()))?;
    rl.set_helper(Some(KataHelper {
        interp: Rc::clone(&interp),
    }));

    // Ensure data directory exists, load history.
    let hist = history_path();
    if let Some(parent) = hist.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = rl.load_history(&hist);

    println!("{} {}", "kata".cyan().bold(), "(Ctrl+D to exit)".dimmed());

    let mut accumulated = String::new();
    let mut continuation = false;

    loop {
        let prompt = if continuation { "· " } else { "λ " };
        match rl.readline(prompt) {
            Ok(line) => {
                if !accumulated.is_empty() {
                    accumulated.push('\n');
                }
                accumulated.push_str(&line);

                if !is_balanced(&accumulated) {
                    continuation = true;
                    continue;
                }

                let input = accumulated.trim().to_string();
                if !input.is_empty() {
                    let _ = rl.add_history_entry(&input);
                    execute(&mut interp.borrow_mut(), &input);
                }
                accumulated.clear();
                continuation = false;
            }
            Err(ReadlineError::Interrupted) => {
                if continuation {
                    accumulated.clear();
                    continuation = false;
                    println!();
                }
            }
            Err(ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("{} {e}", "error:".red());
                break;
            }
        }
    }

    let _ = rl.save_history(&hist);
    info!("REPL exited");
    Ok(())
}
