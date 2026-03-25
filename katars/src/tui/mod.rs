use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Config, EditMode, Editor, Helper};
use std::borrow::Cow;
use tracing::info;

use crate::ks;

// ── Completion helper ────────────────────────────────────────────────────────

const KEYWORDS: &[&str] = &[
    "bail", "cont", "else", "elif", "enum", "false", "for", "func", "if", "impl", "import", "in",
    "kind", "let", "match", "nil", "ret", "self", "Self", "true", "type", "typeof", "unsafe",
    "while", "with",
];

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
        // Find the start of the current word (including dots for paths).
        let start = line[..pos]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
            .map_or(0, |i| i + 1);
        let token = &line[start..pos];
        if token.is_empty() {
            return Ok((start, Vec::new()));
        }

        let interp = self.interp.borrow();

        // Check for dot-path completion: "std." or "std.mem."
        if let Some(dot_pos) = token.rfind('.') {
            let receiver = &token[..dot_pos];
            let attr_prefix = &token[dot_pos + 1..];

            // Walk the dot-path to resolve the receiver.
            let attrs = self.resolve_dot_completions(&interp, receiver);

            let matches: Vec<Pair> = attrs
                .into_iter()
                .filter(|name| name.starts_with(attr_prefix))
                .map(|name| Pair {
                    display: name.clone(),
                    replacement: name,
                })
                .collect();

            // Replace only the part after the last dot.
            let replace_start = start + dot_pos + 1;
            return Ok((replace_start, matches));
        }

        // Simple name completion: keywords + scope names.
        let scope_names = interp.visible_names();
        let mut matches: Vec<Pair> = KEYWORDS
            .iter()
            .map(|s| s.to_string())
            .chain(scope_names.into_iter())
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

impl KataHelper {
    /// Resolve a dot-separated path and return completions for the final segment.
    fn resolve_dot_completions(&self, interp: &ks::Interpreter, receiver: &str) -> Vec<String> {
        // Split "std.mem" into ["std", "mem"] and walk segment by segment.
        let segments: Vec<&str> = receiver.split('.').collect();
        if segments.is_empty() {
            return Vec::new();
        }
        // Get completions for the root name first.
        let mut attrs = interp.completions_for(segments[0]);
        // For deeper paths like "std.mem", we need the interpreter to resolve
        // "std" then "mem" within that module. For now, only support one level
        // of dot-completion by asking completions_for the root.
        // TODO: support arbitrary depth by walking the module tree.
        if segments.len() == 1 {
            return attrs;
        }
        // For multi-segment paths, try to resolve through the module tree.
        // This handles "std.mem.<tab>" by looking at the mem module's entries.
        attrs = interp.completions_for_path(&segments);
        attrs
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
        if prompt.contains('·') {
            Cow::Owned(format!("\x1b[90m{prompt}\x1b[0m"))
        } else {
            Cow::Owned(format!("\x1b[36;1m{prompt}\x1b[0m"))
        }
    }
}

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

    // Persistent interpreter with prelude.
    let interp = Rc::new(RefCell::new(ks::Interpreter::new()));
    {
        let prelude_src = include_str!("../../../std/prelude.ks");
        if let Ok(prelude) = ks::parse(prelude_src, "<prelude>") {
            let mut sink = Vec::new();
            let _ = interp.borrow_mut().exec_program(&prelude, None, &mut sink);
        }
    }

    let config = Config::builder().edit_mode(EditMode::Emacs).build();
    let mut rl = Editor::with_config(config).map_err(|e| io::Error::other(e.to_string()))?;
    rl.set_helper(Some(KataHelper {
        interp: Rc::clone(&interp),
    }));
    let _ = rl.load_history(&history_path());

    println!("\x1b[36;1mkata\x1b[0m \x1b[90m(Ctrl+D to exit)\x1b[0m");

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
