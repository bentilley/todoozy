use super::SyntaxRule;

pub const GO: [SyntaxRule; 3] = [
    SyntaxRule::LineComment("//"),
    SyntaxRule::BlockComment("/*", "*/"),
    SyntaxRule::RawString("`", "`"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_todos() {
        let parser = crate::lang::Parser::new(&GO);

        // Todo as line comments
        let text = r#"
    some := "code

    // TODO 2020-08-06 Can it handle line comments? +Testing
    //
    // This is the description.
    more := "code"
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
    if some := "code"; some == "code" {
        /* TODO 2020-08-06 Can it handle block comments? +Testing

           This is the description.
         */
        more := "code"
    }
"#;
        assert_eq!(
            parser.parse_todos(text)[0],
            (
                3 as usize,
                6 as usize,
                r#"2020-08-06 Can it handle block comments? +Testing

This is the description."#
                    .to_string()
            )
        );

        // Todo as block comment (end token on last line of todo)
        let text = r#"
    for some := range "code" {
        /* TODO 2020-08-06 Can it handle block comments? +Testing

           This is the description. */
        more := "code"
    }
"#;
        assert_eq!(
            parser.parse_todos(text)[0],
            (
                3 as usize,
                5 as usize,
                r#"2020-08-06 Can it handle block comments? +Testing

This is the description."#
                    .to_string()
            )
        );

        // Todo with indented lines
        let text = r#"
    some := "code"

    /* TODO 2020-08-06 Can it handle indented todos? +Testing

       This is a test todo with some indented lines:
         - This is an even more indented line.
     */

    more := "code
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
    some := "code"
    text := `
        // TODO 2020-08-06 Can it handle this fake todo? +Testing
        //
        // This todo is in a raw string, so ignore it.
    `

    // TODO 2020-08-06 Does it find the real todo? +Testing
    //
    // This todo isn't in a raw string.

    more := "code"
"##;
        assert_eq!(parser.parse_todos(text).len(), 1);
        assert_eq!(
            parser.parse_todos(text)[0],
            (
                9 as usize,
                11 as usize,
                r#"2020-08-06 Does it find the real todo? +Testing

This todo isn't in a raw string."#
                    .to_string()
            )
        );
    }
}
