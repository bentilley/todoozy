use super::SyntaxRule;

pub const RUST: [SyntaxRule; 7] = [
    SyntaxRule::LineComment(b"//!"),
    SyntaxRule::LineComment(b"///"),
    SyntaxRule::LineComment(b"//"),
    SyntaxRule::BlockComment(b"/*", b"*/"),
    SyntaxRule::SkipDelimited(b"r#\"", b"\"#"),
    SyntaxRule::SkipDelimited(b"r##\"", b"\"##"),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
];

#[cfg(test)]
mod tests {
    use super::*;

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
            parser.parse_todos(text)[0],
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
            parser.parse_todos(text)[0],
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
            parser.parse_todos(text)[0],
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
            parser.parse_todos(text)[0],
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
        assert_eq!(parser.parse_todos(text).len(), 1);
        assert_eq!(
            parser.parse_todos(text)[0],
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
        let todos = parser.parse_todos(text);
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
        let todos = parser.parse_todos(text);
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
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }
}
