use super::*;

#[test]
fn widths_follow_the_letters() {
    assert!(width("mmmm", 10.0, false) > width("iiii", 10.0, false) * 3.0);
    assert!(width("Title", 10.0, true) > width("Title", 10.0, false));
    assert_eq!(width("", 10.0, false), 0.0);
    assert!((width("Hello", 10.0, false) - 23.46).abs() < 0.01);
}

#[test]
fn wrapping_keeps_words_and_line_breaks() {
    let lines = wrap("The quick brown fox\njumps", 60.0, 10.0, false);
    assert_eq!(lines, ["The quick", "brown fox", "jumps"]);
    assert!(wrap("  ", 60.0, 10.0, false).is_empty());
    let split = wrap("abcdefghijklmnopqrstuvwxyz", 60.0, 10.0, false);
    assert!(split.len() > 1 && split.iter().all(|l| width(l, 10.0, false) <= 60.0));
    assert_eq!(split.concat(), "abcdefghijklmnopqrstuvwxyz");
}

#[test]
fn cutting_says_so() {
    assert_eq!(clamp_lines(vec!["a".into(), "b".into(), "c".into()], 2), ["a", "b…"]);
    assert_eq!(fit("short", 100.0, 10.0, false), "short");
    let cut = fit("a long title that will not fit", 60.0, 10.0, false);
    assert!(cut.ends_with('…') && width(&cut, 10.0, false) <= 60.0, "{cut}");
}
