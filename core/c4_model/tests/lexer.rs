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
