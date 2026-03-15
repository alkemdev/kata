pub mod ast;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod types;
pub mod value;

pub use ast::Program;
pub use interpreter::Interpreter;
pub use lexer::SpannedToken;

/// Lex `source` into a token stream. Always succeeds; lex errors appear as
/// `Token::Error` entries in the result.
pub fn lex(source: &str) -> Vec<SpannedToken> {
    lexer::lex(source)
}

/// Parse `source` into an AST. Errors are printed to stderr via ariadne.
pub fn parse(source: &str, filename: &str) -> Result<Program, ()> {
    parser::parse(source, filename)
}

/// The KataScript standard prelude, embedded at compile time.
const PRELUDE_SRC: &str = include_str!("../../../std/prelude.ks");

/// Run `source`: parse then evaluate. Errors go to stderr.
pub fn run(source: &str, filename: &str) -> Result<(), ()> {
    let prelude = parse(PRELUDE_SRC, "<prelude>").map_err(|()| {
        eprintln!("fatal: failed to parse standard prelude");
    })?;
    let program = parse(source, filename)?;
    let mut interp = Interpreter::new();
    interp
        .exec_program(&program, Some(&prelude), &mut std::io::stdout())
        .map_err(|e| eprintln!("runtime error: {e}"))
}
