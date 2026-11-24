//! Source text to tokens.

/// Where something is in a source file. Lines and columns count from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub file: usize,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Str(String),
    /// `#name`: a tag, or a hex color.
    Hash(String),
    /// `10`, `0.5`, `10%`.
    Num(String),
    /// Something taken verbatim up to the next whitespace: a URL or a path.
    Raw(String),
    /// `->`
    Arrow,
    /// `<->`
    BiArrow,
    /// `<-`
    BackArrow,
    /// `-[`
    ArrowOpen,
    /// `]->`
    ArrowClose,
    Dot,
    Star,
    LBrace,
    RBrace,
    Eq,
    Ne,
    Colon,
    Semi,
    Comma,
    Eof,
}

impl Tok {
    /// How a token reads in an error message.
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(s) => format!("'{s}'"),
            Tok::Str(_) => "a string".into(),
            Tok::Hash(s) => format!("'#{s}'"),
            Tok::Num(s) | Tok::Raw(s) => format!("'{s}'"),
            Tok::Arrow => "'->'".into(),
            Tok::BiArrow => "'<->'".into(),
            Tok::BackArrow => "'<-'".into(),
            Tok::ArrowOpen => "'-['".into(),
            Tok::ArrowClose => "']->'".into(),
            Tok::Dot => "'.'".into(),
            Tok::Star => "'*'".into(),
            Tok::LBrace => "'{'".into(),
            Tok::RBrace => "'}'".into(),
            Tok::Eq => "'='".into(),
            Tok::Ne => "'!='".into(),
            Tok::Colon => "':'".into(),
            Tok::Semi => "';'".into(),
            Tok::Comma => "','".into(),
            Tok::Eof => "the end of the file".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
    /// Whether whitespace (or a comment) came right before it.
    pub spaced: bool,
}

pub struct LexError {
    pub span: Span,
    pub message: String,
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Never fails: what cannot be read is reported and skipped.
pub fn lex(file: usize, text: &str) -> (Vec<Token>, Vec<LexError>) {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let (mut i, mut line, mut col) = (0usize, 1u32, 1u32);
    let mut spaced = true;

    macro_rules! advance {
        () => {{
            if chars[i] == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
            i += 1;
        }};
    }

    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied().unwrap_or('\0');
        if c.is_whitespace() {
            advance!();
            spaced = true;
            continue;
        }
        if c == '/' && next == '/' {
            while i < chars.len() && chars[i] != '\n' {
                advance!();
            }
            spaced = true;
            continue;
        }
        if c == '/' && next == '*' {
            let start = Span { file, line, col };
            advance!();
            advance!();
            let from = i;
            let mut closed = false;
            while i < chars.len() {
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    advance!();
                    advance!();
                    closed = true;
                    break;
                }
                advance!();
            }
            if !closed {
                errors.push(LexError { span: start, message: "This comment is never closed.".into() });
            }
            // How LikeC4's editor saves a view arranged by hand.
            if chars[from..i].iter().collect::<String>().contains("@likec4-generated") {
                errors.push(LexError {
                    span: start,
                    message: "Manual layout is not supported yet; this view is laid out automatically."
                        .into(),
                });
            }
            spaced = true;
            continue;
        }

        let span = Span { file, line, col };
        let mut push = |tok: Tok, spaced: &mut bool| {
            tokens.push(Token { tok, span, spaced: *spaced });
            *spaced = false;
        };

        // Strings, in either quote, over as many lines as they like.
        if c == '\'' || c == '"' {
            advance!();
            let mut s = String::new();
            let mut closed = false;
            while i < chars.len() {
                let d = chars[i];
                if d == '\\' && i + 1 < chars.len() {
                    advance!();
                    s.push(match chars[i] {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                    advance!();
                    continue;
                }
                advance!();
                if d == c {
                    closed = true;
                    break;
                }
                s.push(d);
            }
            if !closed {
                errors.push(LexError { span, message: "This string is never closed.".into() });
            }
            push(Tok::Str(s), &mut spaced);
            continue;
        }

        // A URL (`https://...`) or a relative path (`./docs`, `../docs`), verbatim.
        let rest_starts =
            |at: usize, what: &str| what.chars().enumerate().all(|(k, w)| chars.get(at + k) == Some(&w));
        let scheme_end = {
            let mut k = i;
            while k < chars.len() && chars[k].is_ascii_alphabetic() {
                k += 1;
            }
            k
        };
        let is_url = scheme_end > i && rest_starts(scheme_end, "://");
        let is_path = rest_starts(i, "./") || rest_starts(i, "../");
        if is_url || is_path {
            let mut s = String::new();
            while i < chars.len() && !chars[i].is_whitespace() {
                s.push(chars[i]);
                advance!();
            }
            push(Tok::Raw(s), &mut spaced);
            continue;
        }

        if is_ident_start(c) {
            let mut s = String::new();
            while i < chars.len() {
                let d = chars[i];
                // A hyphen belongs to the name only when the name goes on
                // after it: `api-gateway`, but not the `-` of `a->b`.
                let hyphen = d == '-' && chars.get(i + 1).is_some_and(|&e| is_ident_char(e));
                if !(is_ident_char(d) || hyphen) {
                    break;
                }
                s.push(d);
                advance!();
            }
            push(Tok::Ident(s), &mut spaced);
            continue;
        }

        if c.is_ascii_digit() {
            let mut s = String::new();
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '%')
            {
                s.push(chars[i]);
                advance!();
            }
            push(Tok::Num(s), &mut spaced);
            continue;
        }

        if c == '#' {
            advance!();
            let mut s = String::new();
            while i < chars.len() && (is_ident_char(chars[i]) || chars[i] == '-') {
                s.push(chars[i]);
                advance!();
            }
            push(Tok::Hash(s), &mut spaced);
            continue;
        }

        let (tok, len) = match (c, next) {
            ('-', '>') => (Tok::Arrow, 2),
            ('-', '[') => (Tok::ArrowOpen, 2),
            (']', '-') if chars.get(i + 2) == Some(&'>') => (Tok::ArrowClose, 3),
            ('<', '-') if chars.get(i + 2) == Some(&'>') => (Tok::BiArrow, 3),
            ('<', '-') => (Tok::BackArrow, 2),
            ('!', '=') => (Tok::Ne, 2),
            ('=', '=') => (Tok::Eq, 2),
            ('.', _) => (Tok::Dot, 1),
            ('*', _) => (Tok::Star, 1),
            ('{', _) => (Tok::LBrace, 1),
            ('}', _) => (Tok::RBrace, 1),
            ('=', _) => (Tok::Eq, 1),
            (':', _) => (Tok::Colon, 1),
            (';', _) => (Tok::Semi, 1),
            (',', _) => (Tok::Comma, 1),
            _ => {
                errors.push(LexError { span, message: format!("Unexpected character '{c}'.") });
                advance!();
                continue;
            }
        };
        for _ in 0..len {
            advance!();
        }
        push(tok, &mut spaced);
    }
    tokens.push(Token { tok: Tok::Eof, span: Span { file, line, col }, spaced: true });
    (tokens, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(text: &str) -> Vec<Tok> {
        lex(0, text).0.into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn names_arrows_and_strings() {
        assert_eq!(
            toks("api-gateway->db 'reads' // why\n"),
            vec![
                Tok::Ident("api-gateway".into()),
                Tok::Arrow,
                Tok::Ident("db".into()),
                Tok::Str("reads".into()),
                Tok::Eof
            ]
        );
        assert_eq!(
            toks("a -[async]-> b.c.*"),
            vec![
                Tok::Ident("a".into()),
                Tok::ArrowOpen,
                Tok::Ident("async".into()),
                Tok::ArrowClose,
                Tok::Ident("b".into()),
                Tok::Dot,
                Tok::Ident("c".into()),
                Tok::Dot,
                Tok::Star,
                Tok::Eof
            ]
        );
    }

    #[test]
    fn urls_tags_and_numbers() {
        assert_eq!(
            toks("link https://example.com/a?b=1 #next opacity 10% color #ff00aa"),
            vec![
                Tok::Ident("link".into()),
                Tok::Raw("https://example.com/a?b=1".into()),
                Tok::Hash("next".into()),
                Tok::Ident("opacity".into()),
                Tok::Num("10%".into()),
                Tok::Ident("color".into()),
                Tok::Hash("ff00aa".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn positions_and_unclosed_strings() {
        let (tokens, errors) = lex(0, "a\n  'open");
        assert_eq!((tokens[1].span.line, tokens[1].span.col), (2, 3));
        assert_eq!(errors.len(), 1);
    }
}
