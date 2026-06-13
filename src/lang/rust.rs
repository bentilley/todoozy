use super::{Comment, SyntaxRule};

pub const RUST: [SyntaxRule; 7] = [
    SyntaxRule::LineComment(b"//!"),
    SyntaxRule::LineComment(b"///"),
    SyntaxRule::LineComment(b"//"),
    SyntaxRule::BlockComment(b"/*", b"*/"),
    SyntaxRule::Custom(skip_raw_string),
    SyntaxRule::Custom(skip_char_literal),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
];

/// Skips Rust raw strings (`r"..."`, `r#"..."#`, `r##"..."##`, ...) with any
/// number of `#` delimiters, including `b`/`br`-prefixed byte string variants.
fn skip_raw_string<'a>(
    current_byte: u8,
    text: &'a [u8],
    position: usize,
) -> Option<(usize, usize, Option<Comment<'a>>)> {
    if current_byte != b'r' {
        return None;
    }

    let mut hashes = 0;
    while text.get(position + 1 + hashes) == Some(&b'#') {
        hashes += 1;
    }

    if text.get(position + 1 + hashes) != Some(&b'"') {
        return None;
    }

    let len = text.len();
    let mut pos = position + 2 + hashes; // skip past r, #*, "
    let mut lines = 0;

    while pos < len {
        if text[pos] == b'"'
            && pos + 1 + hashes <= len
            && text[pos + 1..pos + 1 + hashes].iter().all(|&b| b == b'#')
        {
            pos += 1 + hashes;
            break;
        }

        if text[pos] == b'\n' {
            lines += 1;
        }
        pos += 1;
    }

    Some((pos - position, lines, None))
}

/// Skips Rust character literals, e.g. `'a'`, `'"'`, `'\''`, `'\n'`,
/// `'\x41'`, `'\u{1F600}'` (and `b'...'` byte literals, since the `b` prefix
/// is just an ordinary byte that this rule doesn't need to see).
///
/// Without this, a lone `'"'` or `'\''` would be mistaken for a string
/// delimiter by `SkipDelimitedWithEscape`, desyncing the rest of the parse.
fn skip_char_literal<'a>(
    current_byte: u8,
    text: &'a [u8],
    position: usize,
) -> Option<(usize, usize, Option<Comment<'a>>)> {
    if current_byte != b'\'' {
        return None;
    }

    if text.get(position + 1) == Some(&b'\\') {
        // Unicode escape: '\u{XXXX}'
        if text.get(position + 2) == Some(&b'u') && text.get(position + 3) == Some(&b'{') {
            let mut end = position + 4;
            while end < text.len() && text[end] != b'}' {
                end += 1;
            }
            return if text.get(end + 1) == Some(&b'\'') {
                Some((end + 2 - position, 0, None))
            } else {
                None
            };
        }

        // Hex byte escape: '\xNN'
        if text.get(position + 2) == Some(&b'x') && text.get(position + 5) == Some(&b'\'') {
            return Some((6, 0, None));
        }

        // Single-character escape: '\n', '\t', '\\', '\'', '\0', ...
        if text.get(position + 3) == Some(&b'\'') {
            return Some((4, 0, None));
        }

        return None;
    }

    // Unescaped single byte, e.g. 'a', '"'. Requires the next byte to not be
    // `'` so that lifetimes like 'a in `&'a [u8]` aren't mistaken for a
    // literal.
    if text.get(position + 1) != Some(&b'\'') && text.get(position + 2) == Some(&b'\'') {
        Some((3, 0, None))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    #[test]
    fn test_parser() {
        let parser = crate::lang::Parser::new("TODO", &RUST);

        // Todo as line comments
        let text = r#"
    let some = "code";

    // TODO 2020-08-06 Can it handle line comments? +Testing
    //
    // This is the description.
    let more = "code";
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                4 as usize,
                6 as usize,
                r#"2020-08-06 Can it handle line comments? +Testing

This is the description."#
                    .to_string()
            )
        );

        // Todo as block comment (end token on new line)
        let text = r#"
    let some = "code";

    /* TODO 2020-08-06 Can it handle block comments? +Testing

       This is the description.
     */
    let more = "code";
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                4 as usize,
                7 as usize,
                r#"2020-08-06 Can it handle block comments? +Testing

This is the description."#
                    .to_string()
            )
        );

        // Todo as block comment (end token on last line of todo)
        let text = r#"
    let some = "code";

    /* TODO 2020-08-06 Can it handle block comments? +Testing

       This is the description. */
    let more = "code";
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                4 as usize,
                6 as usize,
                r#"2020-08-06 Can it handle block comments? +Testing

This is the description."#
                    .to_string()
            )
        );

        // Todo with indented lines
        let text = r#"
    let some = "code";

    /* TODO 2020-08-06 Can it handle indented todos? +Testing

       This is a test todo with some indented lines:
         - This is an even more indented line.
     */

    let more = "code";
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                4 as usize,
                8 as usize,
                r#"2020-08-06 Can it handle indented todos? +Testing

This is a test todo with some indented lines:
  - This is an even more indented line."#
                    .to_string()
            )
        );

        // File with raw strings
        let text = r##"
    let some = "code";
    let text = r#"
        /* TODO 2020-08-06 Can it handle this fake todo? +Testing
         *
         * This todo is in a raw string, so ignore it.
         */
    "#

    /* TODO 2020-08-06 Does it find the real todo? +Testing

       This todo isn't in a raw string.
     */

    let more = "code";
"##;
        assert_eq!(parser.parse_str(text).len(), 1);
        assert_eq!(
            parser.parse_str(text)[0],
            (
                10 as usize,
                13 as usize,
                r#"2020-08-06 Does it find the real todo? +Testing

This todo isn't in a raw string."#
                    .to_string()
            )
        );
    }

    #[test]
    fn todo_inside_regular_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &RUST);
        let text = r#"
let some = "code";
let msg = "// TODO this is inside a string";

// TODO this is a real todo
let more = "code";
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn todo_inside_multiline_regular_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &RUST);
        let text = r#"
let some = "code";
let msg = "hello
// TODO this is inside a multiline string
world";

// TODO this is a real todo
let more = "code";
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_regular_string() {
        let parser = crate::lang::Parser::new("TODO", &RUST);
        let text = r#"
let msg = "hello \"
// TODO false positive
world";

// TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn todo_inside_raw_string_no_hashes_ignored() {
        let parser = crate::lang::Parser::new("TODO", &RUST);
        let text = r#"
let s = r"
// TODO this is inside a raw string
";

// TODO this is a real todo
let more = "code";
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn todo_inside_raw_string_three_hashes_ignored() {
        let parser = crate::lang::Parser::new("TODO", &RUST);
        let text = r####"
let s = r###"
// TODO this is inside a raw string
embedded "## sequence does not end the string
"###;

// TODO this is a real todo
let more = "code";
"####;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn char_literals_with_escapes_do_not_break_parsing() {
        let parser = crate::lang::Parser::new("TODO", &RUST);
        let text = r#"
let a = 'a';
let quote = '"';
let apos = '\'';
let newline = '\n';
let tab = '\t';
let backslash = '\\';
let nul = '\0';
let hex = '\x41';
let unicode = '\u{1F600}';

// TODO this is a real todo
let more = "code";
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn byte_char_literals_do_not_break_parsing() {
        let parser = crate::lang::Parser::new("TODO", &RUST);
        let text = r#"
let a = b'a';
let quote = b'"';
let apos = b'\'';
let newline = b'\n';

// TODO this is a real todo
let more = "code";
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn lifetime_annotations_are_not_mistaken_for_char_literals() {
        let parser = crate::lang::Parser::new("TODO", &RUST);
        let text = r#"
// TODO before lifetimes
fn first_char<'a, 'b>(s: &'a str, _other: &'b str) -> Option<char> {
    if s.starts_with('a') {
        Some('a')
    } else {
        None
    }
}

struct Borrowed<'a> {
    text: &'a str,
}

// TODO after lifetimes
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].2, "before lifetimes".to_string());
        assert_eq!(todos[1].2, "after lifetimes".to_string());
    }

    #[test]
    fn char_literal_with_quote_does_not_desync_raw_string_parsing() {
        let parser = crate::lang::Parser::new("TODO", &RUST);
        let text = r##"
fn quote_byte() -> u8 {
    b'"'
}

// TODO this is a real todo

let s = r#"
// TODO this is inside a raw string
"#;
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }
}
