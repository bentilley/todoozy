use super::SyntaxRule;

pub const DOCKERFILE: [SyntaxRule; 1] = [
    SyntaxRule::LineComment("#"),
];

pub fn extract_todos(text: &str) -> Vec<crate::RawTodo> {
    let parser = super::Parser::new(&DOCKERFILE);
    parser.parse_todos(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_todos() {
        let parser = crate::lang::Parser::new(&DOCKERFILE);

        // Todo as line comments
        let text = r#"
FROM ubuntu:22.04

# TODO 2020-08-06 Can it handle line comments? +Testing
#
# This is the description.
RUN apt-get update
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

        // Todo with indented lines
        let text = r#"
FROM ubuntu:22.04

# TODO 2020-08-06 Can it handle indented todos? +Testing
#
# This is a test todo with some indented lines:
#   - This is an even more indented line.

RUN apt-get update
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

        // Multiple todos
        let text = r#"
# TODO 2020-08-06 First todo +Testing
FROM ubuntu:22.04

# TODO 2020-08-06 Second todo +Testing
RUN apt-get update
"#;
        assert_eq!(parser.parse_todos(text).len(), 2);
        assert_eq!(
            parser.parse_todos(text)[0],
            (2 as usize, 2 as usize, "2020-08-06 First todo +Testing".to_string())
        );
        assert_eq!(
            parser.parse_todos(text)[1],
            (5 as usize, 5 as usize, "2020-08-06 Second todo +Testing".to_string())
        );
    }
}
