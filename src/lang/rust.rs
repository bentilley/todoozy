use super::SyntaxRule;

pub const RUST: [SyntaxRule; 6] = [
    SyntaxRule::LineComment("//!"),
    SyntaxRule::LineComment("///"),
    SyntaxRule::LineComment("//"),
    SyntaxRule::BlockComment("/*", "*/"),
    SyntaxRule::RawString("r#\"", "\"#"),
    SyntaxRule::RawString("r##\"", "\"##"),
];

pub fn extract_todos(text: &str) -> Vec<crate::RawTodo> {
    let parser = super::Parser::new(&RUST);
    parser.parse_todos(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser() {
        let parser = crate::lang::Parser::new(&RUST);

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
     *
     * This is the description.
     */
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

        // Todo as block comment (end token on last line of todo)
        let text = r#"
    let some = "code";

    /* TODO 2020-08-06 Can it handle block comments? +Testing
     *
     * This is the description. */
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
     *
     * This is a test todo with some indented lines:
     *   - This is an even more indented line.
     */

    let more = "code";
"#;
        assert_eq!(
            parser.parse_todos(text)[0],
            (
                4 as usize,
                7 as usize,
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
     *
     * This todo isn't in a raw string.
     */

    let more = "code";
"##;
        assert_eq!(parser.parse_todos(text).len(), 1);
        assert_eq!(
            parser.parse_todos(text)[0],
            (
                10 as usize,
                12 as usize,
                r#"2020-08-06 Does it find the real todo? +Testing

This todo isn't in a raw string."#
                    .to_string()
            )
        );
    }
}
