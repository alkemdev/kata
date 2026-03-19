use logos::Logos;
use serde::{Deserialize, Serialize};
use std::fmt;
use tracing::debug;

/// A segment of a string literal — either literal text or an interpolation expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StringPart {
    /// Literal text segment (escape sequences already resolved).
    Lit(String),
    /// Raw source text of an interpolated expression: `{expr}`.
    Interp(String),
}

/// The complete KataScript token set.
///
/// Whitespace and line comments are silently skipped by the lexer.
/// Unrecognised bytes produce `Err(())` from logos' iterator; callers map
/// those to `Token::Error` so the token stream stays intact for recovery.
#[derive(Logos, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[logos(skip r"[ \t\r\n]+")]
#[logos(skip(r"//[^\n]*", allow_greedy = true))]
pub enum Token {
    // ── literals ─────────────────────────────────────────────────────────
    /// A string literal. Double-quoted strings support `{expr}` interpolation;
    /// single-quoted strings are literal (no interpolation). Both process escapes.
    #[token("\"", lex_double_string)]
    #[token("'", lex_single_string)]
    Str(Vec<StringPart>),

    /// An integer or decimal number (stored as raw text for lossless round-trip).
    #[regex(r"[0-9]+(\.[0-9]+)?", |lex| lex.slice().to_string())]
    Num(String),

    // ── keywords ─────────────────────────────────────────────────────────
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("nil")]
    Nil,
    #[token("let")]
    Let,
    #[token("func")]
    Func,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("elif")]
    Elif,
    #[token("enum")]
    Enum,
    #[token("while")]
    While,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("with")]
    With,
    #[token("kind")]
    Kind,
    #[token("impl")]
    Impl,
    #[token("type")]
    Type,
    #[token("as")]
    As,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("ret")]
    Ret,

    // ── identifiers ──────────────────────────────────────────────────────
    /// Must come after all keyword tokens so keywords are matched first.
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    // ── punctuation ──────────────────────────────────────────────────────
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token(";")]
    Semicolon,
    #[token(":")]
    Colon,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    #[token("=")]
    Eq,
    #[token("==")]
    EqEq,
    #[token("!=")]
    BangEq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("<=")]
    LtEq,
    #[token(">=")]
    GtEq,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("!")]
    Bang,
    #[token("&&")]
    And,
    #[token("||")]
    Or,

    /// Produced synthetically for unrecognised bytes — not by logos directly.
    Error,
}

/// Shared string body scanner. Processes escape sequences and optionally `{expr}`
/// interpolation. Returns `(bytes_consumed_including_close_quote, parts)` or `None`
/// for unterminated strings.
///
/// - `close_quote`: `b'"'` or `b'\''`
/// - `interpolate`: whether `{` triggers interpolation (double-quoted only)
///
/// All special characters (`\`, `{`, `}`, `"`, `'`) are ASCII, so byte-level
/// checks are safe even in multi-byte UTF-8 — continuation bytes (0x80..0xBF)
/// never collide with ASCII. Literal characters are decoded via `str::chars()`
/// to preserve multi-byte codepoints.
fn scan_string_body(
    rest: &str,
    close_quote: u8,
    interpolate: bool,
) -> Option<(usize, Vec<StringPart>)> {
    let bytes = rest.as_bytes();
    let mut parts: Vec<StringPart> = Vec::new();
    let mut lit = String::new();
    let mut i = 0;

    while i < bytes.len() {
        // Closing quote — flush accumulated literal and return.
        if bytes[i] == close_quote {
            if !lit.is_empty() {
                parts.push(StringPart::Lit(lit));
            }
            return Some((i + 1, parts));
        }

        // Escape sequence.
        if bytes[i] == b'\\' {
            i += 1;
            if i >= bytes.len() {
                return None;
            }
            match bytes[i] {
                b'n' => {
                    lit.push('\n');
                    i += 1;
                }
                b't' => {
                    lit.push('\t');
                    i += 1;
                }
                b'\\' => {
                    lit.push('\\');
                    i += 1;
                }
                b'\'' => {
                    lit.push('\'');
                    i += 1;
                }
                b'"' => {
                    lit.push('"');
                    i += 1;
                }
                b'{' if interpolate => {
                    lit.push('{');
                    i += 1;
                }
                b'}' if interpolate => {
                    lit.push('}');
                    i += 1;
                }
                _ => {
                    // Unknown escape — pass through literally. Decode full char.
                    lit.push('\\');
                    let ch = rest[i..].chars().next()?;
                    lit.push(ch);
                    i += ch.len_utf8();
                }
            }
            continue;
        }

        // Interpolation start — flush literal, scan for matching `}`.
        if bytes[i] == b'{' && interpolate {
            if !lit.is_empty() {
                parts.push(StringPart::Lit(std::mem::take(&mut lit)));
            }
            i += 1;
            let start = i;
            let mut depth = 1u32;
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    b'"' => {
                        // Skip nested double-quoted string inside interpolation.
                        i += 1;
                        while i < bytes.len() {
                            match bytes[i] {
                                b'"' => break,
                                b'\\' => i += 1, // skip escaped char
                                _ => {}
                            }
                            i += 1;
                        }
                    }
                    b'\'' => {
                        // Skip nested single-quoted string inside interpolation.
                        i += 1;
                        while i < bytes.len() {
                            match bytes[i] {
                                b'\'' => break,
                                b'\\' => i += 1, // skip escaped char
                                _ => {}
                            }
                            i += 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            if depth != 0 {
                return None; // Unclosed interpolation brace.
            }
            // i is one past the closing '}'. Expression text is [start..i-1].
            let expr_text = std::str::from_utf8(&bytes[start..i - 1]).ok()?;
            parts.push(StringPart::Interp(expr_text.to_string()));
            continue;
        }

        // Literal character — decode full UTF-8 codepoint.
        let ch = rest[i..].chars().next()?;
        lit.push(ch);
        i += ch.len_utf8();
    }

    // Reached end of input without closing quote.
    None
}

/// Logos callback for single-quoted strings. Processes escape sequences but
/// treats `{` as a literal character (no interpolation).
fn lex_single_string(lex: &mut logos::Lexer<Token>) -> Option<Vec<StringPart>> {
    let rest = lex.remainder();
    let (consumed, parts) = scan_string_body(rest, b'\'', false)?;
    lex.bump(consumed);
    Some(parts)
}

/// Logos callback for double-quoted strings. Processes escape sequences and
/// `{expr}` interpolation boundaries.
fn lex_double_string(lex: &mut logos::Lexer<Token>) -> Option<Vec<StringPart>> {
    let rest = lex.remainder();
    let (consumed, parts) = scan_string_body(rest, b'"', true)?;
    lex.bump(consumed);
    Some(parts)
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Str(parts) => {
                write!(f, "\"")?;
                for part in parts {
                    match part {
                        StringPart::Lit(s) => write!(f, "{s}")?,
                        StringPart::Interp(s) => write!(f, "{{{s}}}")?,
                    }
                }
                write!(f, "\"")
            }
            Token::Num(n) => write!(f, "{n}"),
            Token::Ident(s) => write!(f, "{s}"),
            Token::True => write!(f, "true"),
            Token::False => write!(f, "false"),
            Token::Nil => write!(f, "nil"),
            Token::Let => write!(f, "let"),
            Token::Func => write!(f, "func"),
            Token::If => write!(f, "if"),
            Token::Else => write!(f, "else"),
            Token::Elif => write!(f, "elif"),
            Token::Enum => write!(f, "enum"),
            Token::While => write!(f, "while"),
            Token::For => write!(f, "for"),
            Token::In => write!(f, "in"),
            Token::With => write!(f, "with"),
            Token::Kind => write!(f, "kind"),
            Token::Impl => write!(f, "impl"),
            Token::Type => write!(f, "type"),
            Token::As => write!(f, "as"),
            Token::Break => write!(f, "break"),
            Token::Continue => write!(f, "continue"),
            Token::Ret => write!(f, "ret"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::Semicolon => write!(f, ";"),
            Token::Colon => write!(f, ":"),
            Token::Comma => write!(f, ","),
            Token::Dot => write!(f, "."),
            Token::Eq => write!(f, "="),
            Token::EqEq => write!(f, "=="),
            Token::BangEq => write!(f, "!="),
            Token::Lt => write!(f, "<"),
            Token::Gt => write!(f, ">"),
            Token::LtEq => write!(f, "<="),
            Token::GtEq => write!(f, ">="),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Star => write!(f, "*"),
            Token::Slash => write!(f, "/"),
            Token::Bang => write!(f, "!"),
            Token::And => write!(f, "&&"),
            Token::Or => write!(f, "||"),
            Token::Error => write!(f, "<invalid>"),
        }
    }
}

/// A single token annotated with its byte-offset span in the source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpannedToken {
    pub token: Token,
    /// Byte offset of the first character (inclusive).
    pub start: usize,
    /// Byte offset past the last character (exclusive).
    pub end: usize,
}

/// Lex `source` into a flat `Vec<SpannedToken>`.
///
/// Lex errors (unrecognised bytes) are represented in-line as `Token::Error`
/// so callers see a complete token stream and can continue parsing.
pub fn lex(source: &str) -> Vec<SpannedToken> {
    let mut tokens = Vec::new();

    for (result, span) in Token::lexer(source).spanned() {
        let token = match result {
            Ok(tok) => tok,
            Err(()) => {
                let bad = &source[span.clone()];
                tracing::warn!(byte = span.start, ch = %bad, "lex error: unrecognised character");
                Token::Error
            }
        };
        debug!(?token, start = span.start, end = span.end, "token");
        tokens.push(SpannedToken {
            token,
            start: span.start,
            end: span.end,
        });
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convenience: lex and return just the token kinds (no spans).
    fn tokens(src: &str) -> Vec<Token> {
        lex(src).into_iter().map(|st| st.token).collect()
    }

    /// Convenience: lex a single-token source and return it.
    fn one(src: &str) -> Token {
        let toks = tokens(src);
        assert_eq!(toks.len(), 1, "expected exactly one token, got {toks:?}");
        toks.into_iter().next().unwrap()
    }

    /// Shorthand for a plain string token (single Lit part).
    fn str_tok(s: &str) -> Token {
        Token::Str(vec![StringPart::Lit(s.into())])
    }

    // ── literals ─────────────────────────────────────────────────────────────

    #[test]
    fn lex_string() {
        assert_eq!(one(r#""hello, world""#), str_tok("hello, world"));
    }

    #[test]
    fn lex_string_empty() {
        assert_eq!(one(r#""""#), Token::Str(vec![]));
    }

    #[test]
    fn lex_integer() {
        assert_eq!(one("42"), Token::Num("42".into()));
    }

    #[test]
    fn lex_float() {
        assert_eq!(one("3.14"), Token::Num("3.14".into()));
    }

    // ── keywords vs identifiers ───────────────────────────────────────────────

    #[test]
    fn lex_keywords() {
        assert_eq!(one("true"), Token::True);
        assert_eq!(one("false"), Token::False);
        assert_eq!(one("nil"), Token::Nil);
        assert_eq!(one("let"), Token::Let);
        assert_eq!(one("func"), Token::Func);
        assert_eq!(one("if"), Token::If);
        assert_eq!(one("else"), Token::Else);
        assert_eq!(one("enum"), Token::Enum);
        assert_eq!(one("while"), Token::While);
        assert_eq!(one("for"), Token::For);
        assert_eq!(one("in"), Token::In);
        assert_eq!(one("with"), Token::With);
        assert_eq!(one("kind"), Token::Kind);
        assert_eq!(one("impl"), Token::Impl);
        assert_eq!(one("type"), Token::Type);
        assert_eq!(one("as"), Token::As);
        assert_eq!(one("break"), Token::Break);
        assert_eq!(one("continue"), Token::Continue);
        assert_eq!(one("ret"), Token::Ret);
    }

    #[test]
    fn lex_ident() {
        assert_eq!(one("print"), Token::Ident("print".into()));
        assert_eq!(one("x"), Token::Ident("x".into()));
        assert_eq!(one("foo_bar"), Token::Ident("foo_bar".into()));
        assert_eq!(one("_hidden"), Token::Ident("_hidden".into()));
    }

    /// A keyword that is a prefix of an identifier must not be split.
    #[test]
    fn lex_keyword_prefix_is_ident() {
        assert_eq!(one("trueish"), Token::Ident("trueish".into()));
        assert_eq!(one("letter"), Token::Ident("letter".into()));
        // `fn` is no longer a keyword — must lex as an identifier.
        assert_eq!(one("fn"), Token::Ident("fn".into()));
    }

    // ── punctuation ───────────────────────────────────────────────────────────

    #[test]
    fn lex_punctuation() {
        use Token::*;
        assert_eq!(
            tokens("( ) [ ] { } ; : , . = == != < > <= >= + - * / ! && ||"),
            vec![
                LParen, RParen, LBracket, RBracket, LBrace, RBrace, Semicolon, Colon, Comma, Dot,
                Eq, EqEq, BangEq, Lt, Gt, LtEq, GtEq, Plus, Minus, Star, Slash, Bang, And, Or
            ]
        );
    }

    // ── whitespace and comments ───────────────────────────────────────────────

    #[test]
    fn lex_whitespace_skipped() {
        assert_eq!(tokens("  \t\r\n  "), vec![]);
    }

    #[test]
    fn lex_line_comment_skipped() {
        assert_eq!(tokens("// this is a comment"), vec![]);
    }

    #[test]
    fn lex_comment_does_not_eat_next_line() {
        assert_eq!(tokens("// comment\ntrue"), vec![Token::True]);
    }

    // ── error recovery ────────────────────────────────────────────────────────

    #[test]
    fn lex_unknown_char_produces_error_token() {
        let toks = tokens("@");
        assert_eq!(toks, vec![Token::Error]);
    }

    #[test]
    fn lex_error_does_not_halt_lexing() {
        // '@' is invalid but lexing continues and produces the surrounding tokens.
        assert_eq!(
            tokens("true @ false"),
            vec![Token::True, Token::Error, Token::False]
        );
    }

    // ── spans ─────────────────────────────────────────────────────────────────

    #[test]
    fn lex_spans_are_byte_offsets() {
        // "hello" → 0..7 (including the surrounding quotes)
        let toks = lex(r#""hello""#);
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].start, 0);
        assert_eq!(toks[0].end, 7);
    }

    #[test]
    fn lex_spans_sequential() {
        //  p r i n t (  "  h  i  "  )  ;
        //  0 1 2 3 4 5 6  7  8  9 10 11
        let toks = lex(r#"print("hi");"#);
        let spans: Vec<(usize, usize)> = toks.iter().map(|t| (t.start, t.end)).collect();
        assert_eq!(spans, vec![(0, 5), (5, 6), (6, 10), (10, 11), (11, 12)]);
    }

    // ── Display ───────────────────────────────────────────────────────────────

    #[test]
    fn display_roundtrips_operators() {
        assert_eq!(Token::EqEq.to_string(), "==");
        assert_eq!(Token::BangEq.to_string(), "!=");
        assert_eq!(Token::LtEq.to_string(), "<=");
        assert_eq!(Token::GtEq.to_string(), ">=");
        assert_eq!(Token::And.to_string(), "&&");
        assert_eq!(Token::Or.to_string(), "||");
    }

    #[test]
    fn display_ident_and_literals() {
        assert_eq!(Token::Ident("foo".into()).to_string(), "foo");
        assert_eq!(str_tok("hi").to_string(), "\"hi\"");
        assert_eq!(Token::Num("3.14".into()).to_string(), "3.14");
    }

    // ── escape sequences ──────────────────────────────────────────────────────

    #[test]
    fn lex_escape_newline() {
        assert_eq!(one(r#""\n""#), str_tok("\n"));
    }

    #[test]
    fn lex_escape_tab() {
        assert_eq!(one(r#""\t""#), str_tok("\t"));
    }

    #[test]
    fn lex_escape_backslash() {
        assert_eq!(one(r#""\\""#), str_tok("\\"));
    }

    #[test]
    fn lex_escape_quote() {
        assert_eq!(one(r#""\"""#), str_tok("\""));
    }

    #[test]
    fn lex_escape_brace() {
        assert_eq!(one(r#""\{""#), str_tok("{"));
        assert_eq!(one(r#""\}""#), str_tok("}"));
    }

    #[test]
    fn lex_escape_mixed() {
        assert_eq!(one(r#""a\tb\nc""#), str_tok("a\tb\nc"));
    }

    #[test]
    fn lex_string_with_embedded_quote() {
        assert_eq!(one(r#""say \"hi\"""#), str_tok("say \"hi\""));
    }

    // ── single-quoted strings ───────────────────────────────────────────────────

    #[test]
    fn lex_single_quote_basic() {
        assert_eq!(one("'hello'"), str_tok("hello"));
    }

    #[test]
    fn lex_single_quote_empty() {
        assert_eq!(one("''"), Token::Str(vec![]));
    }

    #[test]
    fn lex_single_quote_escapes() {
        assert_eq!(one(r"'\n'"), str_tok("\n"));
        assert_eq!(one(r"'\t'"), str_tok("\t"));
        assert_eq!(one(r"'\\'"), str_tok("\\"));
        assert_eq!(one(r"'\''"), str_tok("'"));
    }

    #[test]
    fn lex_single_quote_no_interpolation() {
        // `{` is literal in single-quoted strings — no Interp parts.
        assert_eq!(one("'{hello}'"), str_tok("{hello}"));
    }

    // ── interpolation (double-quoted) ─────────────────────────────────────────

    #[test]
    fn lex_interp_simple() {
        assert_eq!(
            one(r#""hello {name}""#),
            Token::Str(vec![
                StringPart::Lit("hello ".into()),
                StringPart::Interp("name".into()),
            ])
        );
    }

    #[test]
    fn lex_interp_expr() {
        assert_eq!(
            one(r#""1+1={1+1}""#),
            Token::Str(vec![
                StringPart::Lit("1+1=".into()),
                StringPart::Interp("1+1".into()),
            ])
        );
    }

    #[test]
    fn lex_interp_nested_braces() {
        // Expression containing braces: {Point { x: 1 }}
        assert_eq!(
            one(r#""{Point { x: 1 }}""#),
            Token::Str(vec![StringPart::Interp("Point { x: 1 }".into()),])
        );
    }

    #[test]
    fn lex_interp_nested_string() {
        // Nested string inside interpolation: {"inner"}
        assert_eq!(
            one(r#""outer {"inner"}""#),
            Token::Str(vec![
                StringPart::Lit("outer ".into()),
                StringPart::Interp("\"inner\"".into()),
            ])
        );
    }

    #[test]
    fn lex_interp_multiple() {
        assert_eq!(
            one(r#""{a} and {b}""#),
            Token::Str(vec![
                StringPart::Interp("a".into()),
                StringPart::Lit(" and ".into()),
                StringPart::Interp("b".into()),
            ])
        );
    }

    // ── UTF-8 ────────────────────────────────────────────────────────────────

    #[test]
    fn lex_utf8_double_string() {
        assert_eq!(one("\"héllo 世界\""), str_tok("héllo 世界"));
    }

    #[test]
    fn lex_utf8_single_string() {
        assert_eq!(one("'héllo 世界'"), str_tok("héllo 世界"));
    }

    // ── interpolation edge cases ─────────────────────────────────────────────

    #[test]
    fn lex_interp_empty() {
        // `"{}"` — empty interpolation expression.
        assert_eq!(
            one(r#""{}""#),
            Token::Str(vec![StringPart::Interp("".into())])
        );
    }

    #[test]
    fn lex_interp_single_quote_inside() {
        // Single-quoted string inside interpolation — scanner must track `'...'`.
        assert_eq!(
            one(r#""{'hello'}""#),
            Token::Str(vec![StringPart::Interp("'hello'".into())])
        );
    }

    #[test]
    fn lex_interp_single_quote_with_brace() {
        // `}` inside a single-quoted string inside interpolation must not close the interp.
        assert_eq!(
            one(r#""{func('it}s')}""#),
            Token::Str(vec![StringPart::Interp("func('it}s')".into())])
        );
    }

    #[test]
    fn lex_escaped_backslash_before_brace() {
        // `\\{x}` — escaped backslash followed by real interpolation.
        assert_eq!(
            one(r#""\\{x}""#),
            Token::Str(vec![
                StringPart::Lit("\\".into()),
                StringPart::Interp("x".into()),
            ])
        );
    }

    #[test]
    fn lex_interp_consecutive() {
        // Two interpolations with no literal between them.
        assert_eq!(
            one(r#""{a}{b}""#),
            Token::Str(vec![
                StringPart::Interp("a".into()),
                StringPart::Interp("b".into()),
            ])
        );
    }
}
