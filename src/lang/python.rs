use super::{Parser, SyntaxRule};

const PYTHON: [SyntaxRule; 2] = [
    SyntaxRule::LineComment("#"),
    SyntaxRule::RawString("\"\"\"", "\"\"\""),
    // SyntaxRule::BlockComment("/*", "*/"),
    // String(b"\""),
];

pub fn extract_todos(file_path: &str) -> Vec<(usize, usize, String)> {
    let data = std::fs::read_to_string(file_path).expect("Unable to read file");
    parse_todos(&data)
}

fn parse_todos(text: &str) -> Vec<(usize, usize, String)> {
    let parser = Parser::new(&PYTHON);
    parser.parse_todos(text)
}

#[test]
fn test_parse_todos() {
    // Todo as line comments
    let text = r#"
    some = "code"

    # TODO 2020-08-06 Can it handle line comments? +Testing
    #
    # This is the description.
    more = "code"
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

    // Todo with indented lines
    let text = r#"
    some = "code"

    # TODO 2020-08-06 Can it handle indented todos? +Testing
    #
    # This is a test todo with some indented lines:
    #   - This is an even more indented line.

    more = "code
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
    some = "code"
    text = """
        # TODO 2020-08-06 Can it handle this fake todo? +Testing
        #
        # This todo is in a raw string, so ignore it.
    """

    # TODO 2020-08-06 Does it find the real todo? +Testing
    #
    # This todo isn't in a raw string.

    more = "code"
"##;
    assert_eq!(parse_todos(text).len(), 1);
    assert_eq!(
        parse_todos(text)[0],
        (
            9 as usize,
            11 as usize,
            r#"2020-08-06 Does it find the real todo? +Testing

This todo isn't in a raw string."#
                .to_string()
        )
    );
}
