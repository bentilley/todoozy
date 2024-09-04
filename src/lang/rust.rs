use super::{Parser, SyntaxRule};

const RUST: [SyntaxRule; 5] = [
    SyntaxRule::LineComment("//!"),
    SyntaxRule::LineComment("///"),
    SyntaxRule::LineComment("//"),
    SyntaxRule::BlockComment("/*", "*/"),
    SyntaxRule::RawString("r#\"", "\"#"),
    // String(b"\""),
];

// TODO (C) 2024-09-03 More refactoring as these functions will be copied for each lang +refactor
//
// Goes for extract_todos and parse_todos, the only thing that changes now is the LANG.
pub fn extract_todos(file_path: &str) -> Vec<(usize, usize, String)> {
    let data = std::fs::read_to_string(file_path).expect("Unable to read file");
    parse_todos(&data)
}

fn parse_todos(text: &str) -> Vec<(usize, usize, String)> {
    let parser = Parser::new(&RUST);
    parser.parse_todos(text)
}

#[test]
fn test_parse_todos() {
    // Todo as line comments
    let text = r#"
    let some = "code";

    // TODO 2020-08-06 Can it handle line comments? +Testing
    //
    // This is the description.
    let more = "code";
"#;
    assert_eq!(
        parse_todos(text)[0],
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
        parse_todos(text)[0],
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
        parse_todos(text)[0],
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
        parse_todos(text)[0],
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
    assert_eq!(parse_todos(text).len(), 1);
    assert_eq!(
        parse_todos(text)[0],
        (
            10 as usize,
            12 as usize,
            r#"2020-08-06 Does it find the real todo? +Testing

This todo isn't in a raw string."#
                .to_string()
        )
    );
}
