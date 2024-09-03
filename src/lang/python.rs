use super::{Parser, SyntaxRule};

const PYTHON: [SyntaxRule; 1] = [
    SyntaxRule::LineComment("#"),
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

// TODO (C) 2024-09-03 Stop picking up these tests in the TUI +feature
//
// These tests are showing up in the TUI because they are understood as todos. Either need a way to
// ignore this, or make the comment parser string-aware...
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
    some := "code"

    # TODO 2020-08-06 Can it handle indented todos? +Testing
    #
    # This is a test todo with some indented lines:
    #   - This is an even more indented line.

    more := "code
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
}
