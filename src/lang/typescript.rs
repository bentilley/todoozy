use super::SyntaxRule;

pub const TYPESCRIPT: [SyntaxRule; 3] = [
    SyntaxRule::LineComment(b"//"),
    SyntaxRule::BlockComment(b"/*", b"*/"),
    SyntaxRule::SkipDelimitedWithEscape(b"`", b"`", b'\\'),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser() {
        let parser = crate::lang::Parser::new("TODO", &TYPESCRIPT);

        // Todo as line comments
        let text = r#"
    const some = "code";

    // TODO 2020-08-06 Can it handle line comments? +Testing
    //
    // This is the description.
    const more = "code";
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
    const some = "code";

    /* TODO 2020-08-06 Can it handle block comments? +Testing

       This is the description.
     */
    const more = "code";
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
    const some = "code";

    /* TODO 2020-08-06 Can it handle block comments? +Testing

       This is the description. */
    const more = "code";
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
    const some = "code";

    /* TODO 2020-08-06 Can it handle indented todos? +Testing

       This is a test todo with some indented lines:
         - This is an even more indented line.
     */

    const more = "code";
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
    const some = "code";
    const text = `
        /* TODO 2020-08-06 Can it handle this fake todo? +Testing
         *
         * This todo is in a raw string, so ignore it.
         */
    `

    /* TODO 2020-08-06 Does it find the real todo? +Testing

       This todo isn't in a raw string.
     */

    const more = "code";
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
    fn escaped_backtick_in_template_literal() {
        let parser = crate::lang::Parser::new("TODO", &TYPESCRIPT);
        let text = r##"
const msg = `hello \`
// TODO false positive
world`;

// TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escaped_backslash_before_closing_backtick() {
        // `\\` escapes the backslash, so the backtick ends the string
        let parser = crate::lang::Parser::new("TODO", &TYPESCRIPT);
        let text = r##"
const msg = `hello \\`;

// TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn triple_backslash_before_backtick() {
        // `\\\`` = escaped backslash + escaped backtick, string continues
        let parser = crate::lang::Parser::new("TODO", &TYPESCRIPT);
        let text = r##"
const msg = `hello \\\`
// TODO false positive
world`;

// TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn multiple_escaped_backticks() {
        let parser = crate::lang::Parser::new("TODO", &TYPESCRIPT);
        let text = r##"
const msg = `\`hello\` \`world\``;

// TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escaped_backtick_at_end_of_template() {
        let parser = crate::lang::Parser::new("TODO", &TYPESCRIPT);
        let text = r##"
const msg = `hello \``;

// TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn other_escape_sequences_in_template() {
        // Various escapes shouldn't break parsing or be confused with delimiters
        let parser = crate::lang::Parser::new("TODO", &TYPESCRIPT);
        let text = r##"
const msg = `\n \t \r \' \" \$ \0 \x41 \u0041`;

// TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escape_sequences_mixed_with_escaped_backticks() {
        // Mix of regular escapes and escaped backticks
        let parser = crate::lang::Parser::new("TODO", &TYPESCRIPT);
        let text = r##"
const msg = `line1\nline2\t\`quoted\`\\done`;

// TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }
}
