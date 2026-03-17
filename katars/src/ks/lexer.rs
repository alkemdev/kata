use logos::Logos;
use serde::{Deserialize, Serialize};
use std::fmt;
use tracing::debug;

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
    /// A double-quoted string with no escape processing yet.
    #[regex(r#""[^"]*""#, |lex| lex.slice()[1..lex.slice().len()-1].to_string())]
    Str(String),

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
    #[token("with")]
    With,
    #[token("type")]
    Type,
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

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Str(s) => write!(f, "\"{s}\""),
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
            Token::With => write!(f, "with"),
            Token::Type => write!(f, "type"),
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

    // ── literals ─────────────────────────────────────────────────────────────

    #[test]
    fn lex_string() {
        assert_eq!(one(r#""hello, world""#), Token::Str("hello, world".into()));
    }

    #[test]
    fn lex_string_empty() {
        assert_eq!(one(r#""""#), Token::Str(String::new()));
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
        assert_eq!(one("with"), Token::With);
        assert_eq!(one("type"), Token::Type);
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
        assert_eq!(Token::Str("hi".into()).to_string(), "\"hi\"");
        assert_eq!(Token::Num("3.14".into()).to_string(), "3.14");
    }
}
