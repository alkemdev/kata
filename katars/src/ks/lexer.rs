use logos::Logos;
use serde::{Deserialize, Serialize};
use std::fmt;
use tracing::debug;

/// A segment of a string literal — either literal text or an interpolation expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StringPart {
    /// Literal text segment (escape sequences already resolved).
    Lit(String),
    /// Raw source text of an interpolated expression: `{expr}`.
    /// The usize is the byte offset of the expression start in the original source.
    Interp(String, usize),
}

/// PartialEq ignores the offset on Interp — it's metadata for error reporting,
/// not part of the semantic identity.
impl PartialEq for StringPart {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (StringPart::Lit(a), StringPart::Lit(b)) => a == b,
            (StringPart::Interp(a, _), StringPart::Interp(b, _)) => a == b,
            _ => false,
        }
    }
}

/// A segment of a byte string literal — either raw bytes or an interpolation expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BinPart {
    /// Literal bytes (escape sequences already resolved to raw bytes).
    Bytes(Vec<u8>),
    /// Raw source text of an interpolated expression: `{expr}`.
    /// At runtime, the expression is evaluated, display()'d, and UTF-8 encoded.
    Interp(String, usize),
}

impl PartialEq for BinPart {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (BinPart::Bytes(a), BinPart::Bytes(b)) => a == b,
            (BinPart::Interp(a, _), BinPart::Interp(b, _)) => a == b,
            _ => false,
        }
    }
}

/// The complete KataScript token set.
///
/// Whitespace and line comments are silently skipped by the lexer.
/// Unrecognised bytes produce `Err(())` from logos' iterator; callers map
/// those to `Token::Error` so the token stream stays intact for recovery.
#[derive(Logos, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[logos(skip r"[ \t\r\n]+")]
#[logos(skip(r"#[^\n]*", allow_greedy = true))]
pub enum Token {
    // ── literals ─────────────────────────────────────────────────────────
    /// A string literal. Double-quoted strings support `{expr}` interpolation;
    /// single-quoted strings are literal (no interpolation). Both process escapes.
    #[token("\"", lex_double_string)]
    #[token("'", lex_single_string)]
    Str(Vec<StringPart>),

    /// A byte string literal. `b"..."` supports interpolation; `b'...'` does not.
    /// Escape sequences produce raw bytes (e.g., `\xff` → 1 byte, not UTF-8 of U+00FF).
    #[token("b\"", lex_bin_double)]
    #[token("b'", lex_bin_single)]
    BinStr(Vec<BinPart>),

    /// An integer or decimal number (stored as raw text for lossless round-trip).
    /// Supports decimal, hex (0x), and binary (0b) prefixes.
    #[regex(r"0[xX][0-9a-fA-F]+|0[bB][01]+|[0-9]+(\.[0-9]+)?", |lex| lex.slice().to_string())]
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
    #[token("bail")]
    Bail,
    #[token("cont")]
    Cont,
    #[token("ret")]
    Ret,
    #[token("unsafe")]
    Unsafe,
    #[token("import")]
    Import,
    #[token("match")]
    Match,
    #[token("self")]
    SelfValue,
    #[token("Self")]
    SelfType,

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
    #[token("->")]
    Arrow,
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
    #[token("?")]
    Ques,
    #[token("&&")]
    And,
    #[token("||")]
    Or,
    #[token("@")]
    At,

    /// Produced synthetically for unrecognised bytes — not by logos directly.
    Error,
}

/// Convert an ASCII hex digit to its 4-bit value, or None.
fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
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
    base_offset: usize,
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
                b'r' => {
                    lit.push('\r');
                    i += 1;
                }
                b'0' => {
                    lit.push('\0');
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
                b'x' => {
                    // \xNN — hex byte, exactly 2 hex digits.
                    i += 1;
                    if i + 2 > bytes.len() {
                        return None;
                    }
                    let hi = hex_digit(bytes[i])?;
                    let lo = hex_digit(bytes[i + 1])?;
                    let byte_val = (hi << 4) | lo;
                    lit.push(byte_val as char);
                    i += 2;
                }
                b'u' => {
                    // \uNNNN — Unicode codepoint, exactly 4 hex digits.
                    i += 1;
                    if i + 4 > bytes.len() {
                        return None;
                    }
                    let hex_str = std::str::from_utf8(&bytes[i..i + 4]).ok()?;
                    let codepoint = u32::from_str_radix(hex_str, 16).ok()?;
                    let ch = char::from_u32(codepoint)?;
                    lit.push(ch);
                    i += 4;
                }
                b'U' => {
                    // \UNNNNNNNN — Unicode codepoint, exactly 8 hex digits.
                    i += 1;
                    if i + 8 > bytes.len() {
                        return None;
                    }
                    let hex_str = std::str::from_utf8(&bytes[i..i + 8]).ok()?;
                    let codepoint = u32::from_str_radix(hex_str, 16).ok()?;
                    let ch = char::from_u32(codepoint)?;
                    lit.push(ch);
                    i += 8;
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
            // base_offset + start gives the absolute byte position of the expr
            parts.push(StringPart::Interp(
                expr_text.to_string(),
                base_offset + start,
            ));
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

/// Scan a byte string body, accumulating raw bytes instead of chars.
///
/// Differences from `scan_string_body`:
/// - `\xNN` produces a single raw byte (not a char → UTF-8).
/// - Regular text is UTF-8 encoded to bytes.
/// - `\uNNNN` / `\UNNNNNNNN` produce UTF-8 bytes of the codepoint.
fn scan_bin_body(
    rest: &str,
    close_quote: u8,
    interpolate: bool,
    base_offset: usize,
) -> Option<(usize, Vec<BinPart>)> {
    let bytes = rest.as_bytes();
    let mut parts: Vec<BinPart> = Vec::new();
    let mut lit: Vec<u8> = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == close_quote {
            if !lit.is_empty() {
                parts.push(BinPart::Bytes(lit));
            }
            return Some((i + 1, parts));
        }

        if bytes[i] == b'\\' {
            i += 1;
            if i >= bytes.len() {
                return None;
            }
            match bytes[i] {
                b'n' => {
                    lit.push(b'\n');
                    i += 1;
                }
                b't' => {
                    lit.push(b'\t');
                    i += 1;
                }
                b'r' => {
                    lit.push(b'\r');
                    i += 1;
                }
                b'0' => {
                    lit.push(0);
                    i += 1;
                }
                b'\\' => {
                    lit.push(b'\\');
                    i += 1;
                }
                b'\'' => {
                    lit.push(b'\'');
                    i += 1;
                }
                b'"' => {
                    lit.push(b'"');
                    i += 1;
                }
                b'{' if interpolate => {
                    lit.push(b'{');
                    i += 1;
                }
                b'}' if interpolate => {
                    lit.push(b'}');
                    i += 1;
                }
                b'x' => {
                    // \xNN — raw byte, exactly 2 hex digits.
                    i += 1;
                    if i + 2 > bytes.len() {
                        return None;
                    }
                    let hi = hex_digit(bytes[i])?;
                    let lo = hex_digit(bytes[i + 1])?;
                    lit.push((hi << 4) | lo);
                    i += 2;
                }
                b'u' => {
                    // \uNNNN — UTF-8 encode the codepoint.
                    i += 1;
                    if i + 4 > bytes.len() {
                        return None;
                    }
                    let hex_str = std::str::from_utf8(&bytes[i..i + 4]).ok()?;
                    let codepoint = u32::from_str_radix(hex_str, 16).ok()?;
                    let ch = char::from_u32(codepoint)?;
                    let mut buf = [0u8; 4];
                    lit.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                    i += 4;
                }
                b'U' => {
                    // \UNNNNNNNN — UTF-8 encode the codepoint.
                    i += 1;
                    if i + 8 > bytes.len() {
                        return None;
                    }
                    let hex_str = std::str::from_utf8(&bytes[i..i + 8]).ok()?;
                    let codepoint = u32::from_str_radix(hex_str, 16).ok()?;
                    let ch = char::from_u32(codepoint)?;
                    let mut buf = [0u8; 4];
                    lit.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                    i += 8;
                }
                _ => {
                    lit.push(b'\\');
                    let ch = rest[i..].chars().next()?;
                    let mut buf = [0u8; 4];
                    lit.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                    i += ch.len_utf8();
                }
            }
            continue;
        }

        // Interpolation.
        if bytes[i] == b'{' && interpolate {
            if !lit.is_empty() {
                parts.push(BinPart::Bytes(std::mem::take(&mut lit)));
            }
            i += 1;
            let start = i;
            let mut depth = 1u32;
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    b'"' => {
                        i += 1;
                        while i < bytes.len() {
                            match bytes[i] {
                                b'"' => break,
                                b'\\' => i += 1,
                                _ => {}
                            }
                            i += 1;
                        }
                    }
                    b'\'' => {
                        i += 1;
                        while i < bytes.len() {
                            match bytes[i] {
                                b'\'' => break,
                                b'\\' => i += 1,
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
                return None;
            }
            let expr_text = std::str::from_utf8(&bytes[start..i - 1]).ok()?;
            parts.push(BinPart::Interp(expr_text.to_string(), base_offset + start));
            continue;
        }

        // Regular text — encode as UTF-8 bytes.
        let ch = rest[i..].chars().next()?;
        let mut buf = [0u8; 4];
        lit.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        i += ch.len_utf8();
    }

    None
}

/// Logos callback for single-quoted strings. Processes escape sequences but
/// treats `{` as a literal character (no interpolation).
fn lex_single_string(lex: &mut logos::Lexer<Token>) -> Option<Vec<StringPart>> {
    let rest = lex.remainder();
    let base = lex.span().end; // byte offset after the opening quote
    let (consumed, parts) = scan_string_body(rest, b'\'', false, base)?;
    lex.bump(consumed);
    Some(parts)
}

/// Logos callback for double-quoted strings. Processes escape sequences and
/// `{expr}` interpolation boundaries.
fn lex_double_string(lex: &mut logos::Lexer<Token>) -> Option<Vec<StringPart>> {
    let rest = lex.remainder();
    let base = lex.span().end; // byte offset after the opening quote
    let (consumed, parts) = scan_string_body(rest, b'"', true, base)?;
    lex.bump(consumed);
    Some(parts)
}

/// Logos callback for `b'...'` — byte string, no interpolation.
fn lex_bin_single(lex: &mut logos::Lexer<Token>) -> Option<Vec<BinPart>> {
    let rest = lex.remainder();
    let base = lex.span().end;
    let (consumed, parts) = scan_bin_body(rest, b'\'', false, base)?;
    lex.bump(consumed);
    Some(parts)
}

/// Logos callback for `b"..."` — byte string with interpolation.
fn lex_bin_double(lex: &mut logos::Lexer<Token>) -> Option<Vec<BinPart>> {
    let rest = lex.remainder();
    let base = lex.span().end;
    let (consumed, parts) = scan_bin_body(rest, b'"', true, base)?;
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
                        StringPart::Interp(s, _) => write!(f, "{{{s}}}")?,
                    }
                }
                write!(f, "\"")
            }
            Token::BinStr(parts) => {
                write!(f, "b\"")?;
                for part in parts {
                    match part {
                        BinPart::Bytes(bs) => {
                            for &b in bs {
                                match b {
                                    0x20..=0x7e if b != b'\\' && b != b'"' => {
                                        write!(f, "{}", b as char)?
                                    }
                                    b'\n' => write!(f, "\\n")?,
                                    b'\t' => write!(f, "\\t")?,
                                    b'\r' => write!(f, "\\r")?,
                                    0 => write!(f, "\\0")?,
                                    _ => write!(f, "\\x{b:02x}")?,
                                }
                            }
                        }
                        BinPart::Interp(s, _) => write!(f, "{{{s}}}")?,
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
            Token::Bail => write!(f, "bail"),
            Token::Cont => write!(f, "cont"),
            Token::Ret => write!(f, "ret"),
            Token::Unsafe => write!(f, "unsafe"),
            Token::Import => write!(f, "import"),
            Token::Match => write!(f, "match"),
            Token::SelfValue => write!(f, "self"),
            Token::SelfType => write!(f, "Self"),
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
            Token::Arrow => write!(f, "->"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Star => write!(f, "*"),
            Token::Slash => write!(f, "/"),
            Token::Bang => write!(f, "!"),
            Token::Ques => write!(f, "?"),
            Token::And => write!(f, "&&"),
            Token::Or => write!(f, "||"),
            Token::At => write!(f, "@"),
            Token::Error => write!(f, "<invalid>"),
        }
    }
}

/// A single token annotated with its byte-offset span in the source.
/// All keyword strings, derived from the Token enum.
/// Single source of truth — used by the REPL for tab completion.
pub const KEYWORDS: &[&str] = &[
    "as", "bail", "cont", "elif", "else", "enum", "false", "for", "func", "if", "impl", "import",
    "in", "kind", "let", "match", "nil", "ret", "self", "Self", "true", "type", "unsafe", "while",
    "with",
];

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
                tracing::trace!(byte = span.start, ch = %bad, "lex error: unrecognised character");
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
        assert_eq!(one("bail"), Token::Bail);
        assert_eq!(one("cont"), Token::Cont);
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
        assert_eq!(tokens("# this is a comment"), vec![]);
    }

    #[test]
    fn lex_comment_does_not_eat_next_line() {
        assert_eq!(tokens("# comment\ntrue"), vec![Token::True]);
    }

    // ── error recovery ────────────────────────────────────────────────────────

    #[test]
    fn lex_unknown_char_produces_error_token() {
        let toks = tokens("💀");
        assert_eq!(toks, vec![Token::Error]);
    }

    #[test]
    fn lex_error_does_not_halt_lexing() {
        // '💀' is invalid but lexing continues and produces the surrounding tokens.
        assert_eq!(
            tokens("true 💀 false"),
            vec![Token::True, Token::Error, Token::False]
        );
    }

    #[test]
    fn lex_at_sign() {
        assert_eq!(one("@"), Token::At);
        assert_eq!(tokens("@ T"), vec![Token::At, Token::Ident("T".into())]);
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

    // ── hex byte escapes ──────────────────────────────────────────────────────

    #[test]
    fn lex_escape_hex_byte() {
        assert_eq!(one(r#""\x41""#), str_tok("A"));
        assert_eq!(one(r#""\x61""#), str_tok("a"));
        assert_eq!(one(r#""\x0a""#), str_tok("\n"));
    }

    #[test]
    fn lex_escape_hex_byte_in_context() {
        assert_eq!(one(r#""hi\x21""#), str_tok("hi!"));
    }

    // ── unicode escapes ──────────────────────────────────────────────────────

    #[test]
    fn lex_escape_unicode_4() {
        assert_eq!(one(r#""\u0041""#), str_tok("A"));
        assert_eq!(one(r#""\u2764""#), str_tok("\u{2764}")); // ❤
    }

    #[test]
    fn lex_escape_unicode_8() {
        assert_eq!(one(r#""\U0001f600""#), str_tok("\u{1f600}")); // 😀
    }

    #[test]
    fn lex_escape_unicode_in_context() {
        assert_eq!(one(r#""hello \u2764""#), str_tok("hello \u{2764}")); // ❤
    }

    // ── null escape ──────────────────────────────────────────────────────────

    #[test]
    fn lex_escape_null() {
        assert_eq!(one(r#""\0""#), str_tok("\0"));
    }

    // ── carriage return escape ───────────────────────────────────────────────

    #[test]
    fn lex_escape_carriage_return() {
        assert_eq!(one(r#""\r""#), str_tok("\r"));
        assert_eq!(one(r#""\r\n""#), str_tok("\r\n"));
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
                StringPart::Interp("name".into(), 0),
            ])
        );
    }

    #[test]
    fn lex_interp_expr() {
        assert_eq!(
            one(r#""1+1={1+1}""#),
            Token::Str(vec![
                StringPart::Lit("1+1=".into()),
                StringPart::Interp("1+1".into(), 0),
            ])
        );
    }

    #[test]
    fn lex_interp_nested_braces() {
        // Expression containing braces: {Point { x: 1 }}
        assert_eq!(
            one(r#""{Point { x: 1 }}""#),
            Token::Str(vec![StringPart::Interp("Point { x: 1 }".into(), 0),])
        );
    }

    #[test]
    fn lex_interp_nested_string() {
        // Nested string inside interpolation: {"inner"}
        assert_eq!(
            one(r#""outer {"inner"}""#),
            Token::Str(vec![
                StringPart::Lit("outer ".into()),
                StringPart::Interp("\"inner\"".into(), 0),
            ])
        );
    }

    #[test]
    fn lex_interp_multiple() {
        assert_eq!(
            one(r#""{a} and {b}""#),
            Token::Str(vec![
                StringPart::Interp("a".into(), 0),
                StringPart::Lit(" and ".into()),
                StringPart::Interp("b".into(), 0),
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
            Token::Str(vec![StringPart::Interp("".into(), 0)])
        );
    }

    #[test]
    fn lex_interp_single_quote_inside() {
        // Single-quoted string inside interpolation — scanner must track `'...'`.
        assert_eq!(
            one(r#""{'hello'}""#),
            Token::Str(vec![StringPart::Interp("'hello'".into(), 0)])
        );
    }

    #[test]
    fn lex_interp_single_quote_with_brace() {
        // `}` inside a single-quoted string inside interpolation must not close the interp.
        assert_eq!(
            one(r#""{func('it}s')}""#),
            Token::Str(vec![StringPart::Interp("func('it}s')".into(), 0)])
        );
    }

    #[test]
    fn lex_escaped_backslash_before_brace() {
        // `\\{x}` — escaped backslash followed by real interpolation.
        assert_eq!(
            one(r#""\\{x}""#),
            Token::Str(vec![
                StringPart::Lit("\\".into()),
                StringPart::Interp("x".into(), 0),
            ])
        );
    }

    #[test]
    fn lex_interp_consecutive() {
        // Two interpolations with no literal between them.
        assert_eq!(
            one(r#""{a}{b}""#),
            Token::Str(vec![
                StringPart::Interp("a".into(), 0),
                StringPart::Interp("b".into(), 0),
            ])
        );
    }

    // ── byte string literals ─────────────────────────────────────────────────

    fn bin_tok(bs: &[u8]) -> Token {
        Token::BinStr(vec![BinPart::Bytes(bs.to_vec())])
    }

    #[test]
    fn lex_bin_single_basic() {
        assert_eq!(one("b'hello'"), bin_tok(b"hello"));
    }

    #[test]
    fn lex_bin_double_basic() {
        assert_eq!(one(r#"b"hello""#), bin_tok(b"hello"));
    }

    #[test]
    fn lex_bin_empty() {
        assert_eq!(one("b''"), Token::BinStr(vec![]));
        assert_eq!(one(r#"b"""#), Token::BinStr(vec![]));
    }

    #[test]
    fn lex_bin_hex_escape() {
        // \xff produces a single raw byte 0xFF, not UTF-8 of U+00FF.
        assert_eq!(one(r"b'\xff'"), bin_tok(&[0xff]));
        assert_eq!(one(r"b'\xff\x00\xab'"), bin_tok(&[0xff, 0x00, 0xab]));
    }

    #[test]
    fn lex_bin_named_escapes() {
        assert_eq!(one(r"b'\n\t\r\0'"), bin_tok(&[b'\n', b'\t', b'\r', 0]));
    }

    #[test]
    fn lex_bin_unicode_escape() {
        // \u2764 (❤) in a byte string produces its UTF-8 encoding: 3 bytes.
        assert_eq!(one(r"b'\u2764'"), bin_tok(&[0xe2, 0x9d, 0xa4]));
    }

    #[test]
    fn lex_bin_interp() {
        assert_eq!(
            one(r#"b"hi {name}""#),
            Token::BinStr(vec![
                BinPart::Bytes(b"hi ".to_vec()),
                BinPart::Interp("name".into(), 0),
            ])
        );
    }

    #[test]
    fn lex_bin_no_interp_single() {
        // Single-quoted byte string: {name} is literal, not interpolation.
        assert_eq!(one("b'{name}'"), bin_tok(b"{name}"));
    }

    #[test]
    fn lex_b_space_string_is_ident() {
        // b followed by space + quote: b is an ident, string is separate.
        assert_eq!(
            tokens("b 'hello'"),
            vec![
                Token::Ident("b".into()),
                Token::Str(vec![StringPart::Lit("hello".into())])
            ]
        );
    }
}
