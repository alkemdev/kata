use std::io;

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Config, EditMode, Editor, Helper};
use tracing::info;

use crate::ks;

// ── Completion helper ────────────────────────────────────────────────────────

const KEYWORDS: &[&str] = &[
    "bail", "cont", "else", "elif", "enum", "false", "for", "func", "if", "impl", "import", "in",
    "kind", "let", "match", "nil", "ret", "self", "Self", "true", "type", "typeof", "unsafe",
    "while", "with",
];

struct KataHelper;

impl Completer for KataHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        // Find the start of the current word.
        let start = line[..pos]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map_or(0, |i| i + 1);
        let prefix = &line[start..pos];
        if prefix.is_empty() {
            return Ok((start, Vec::new()));
        }

        let matches: Vec<Pair> = KEYWORDS
            .iter()
            .filter(|kw| kw.starts_with(prefix))
            .map(|kw| Pair {
                display: kw.to_string(),
                replacement: kw.to_string(),
            })
            .collect();

        Ok((start, matches))
    }
}

impl Hinter for KataHelper {
    type Hint = String;
}
impl Highlighter for KataHelper {}
impl Validator for KataHelper {}
impl Helper for KataHelper {}

// ── Multi-line detection ─────────────────────────────────────────────────────

/// Check if input has balanced delimiters. Unbalanced means more input needed.
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
        Err(()) => eprintln!("\x1b[31merror:\x1b[0m parse error"),
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
                    let msg = e.kind.format_with(interp.type_registry());
                    eprintln!("\x1b[31merror:\x1b[0m {msg}");
                }
            }
        }
    }
}

// ── History path ─────────────────────────────────────────────────────────────

fn history_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".kata_history")
}

// ── REPL entry point ─────────────────────────────────────────────────────────

pub fn run_repl() -> io::Result<()> {
    info!("starting REPL");

    let config = Config::builder().edit_mode(EditMode::Emacs).build();
    let mut rl = Editor::with_config(config).map_err(|e| io::Error::other(e.to_string()))?;
    rl.set_helper(Some(KataHelper));
    let _ = rl.load_history(&history_path());

    // Persistent interpreter with prelude.
    let mut interp = ks::Interpreter::new();
    {
        let prelude_src = include_str!("../../../std/prelude.ks");
        if let Ok(prelude) = ks::parse(prelude_src, "<prelude>") {
            let mut sink = Vec::new();
            let _ = interp.exec_program(&prelude, None, &mut sink);
        }
    }

    println!("KataScript REPL (Ctrl+D to exit)");

    let mut accumulated = String::new();
    let mut continuation = false;

    loop {
        let prompt = if continuation { "... " } else { "ks> " };
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
                    execute(&mut interp, &input);
                }
                accumulated.clear();
                continuation = false;
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C: cancel current input
                if continuation {
                    accumulated.clear();
                    continuation = false;
                    println!();
                }
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D: exit
                break;
            }
            Err(e) => {
                eprintln!("error: {e}");
                break;
            }
        }
    }

    let _ = rl.save_history(&history_path());
    info!("REPL exited");
    Ok(())
}
