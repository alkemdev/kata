pub mod ast;
pub mod eval;
pub mod lexer;
pub mod parser;

pub use ast::Program;
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

/// Run `source`: parse then evaluate. Errors go to stderr.
pub fn run(source: &str, filename: &str) -> Result<(), ()> {
    let program = parse(source, filename)?;
    eval::exec_program(&program).map_err(|e| eprintln!("runtime error: {e}"))
}
