use super::sh::skip_escaped_single_quote;
use super::SyntaxRule;

pub const DOCKERFILE: [SyntaxRule; 4] = [
    SyntaxRule::LineComment(b"#"),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
    SyntaxRule::Custom(skip_escaped_single_quote), // must come before SkipDelimited for '
    SyntaxRule::SkipDelimited(b"'", b"'"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    #[test]
    fn test_parse_todos() {
        let parser = crate::lang::Parser::new("TODO", &DOCKERFILE);

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
            (
                2 as usize,
                2 as usize,
                "2020-08-06 First todo +Testing".to_string()
            )
        );
        assert_eq!(
            parser.parse_todos(text)[1],
            (
                5 as usize,
                5 as usize,
                "2020-08-06 Second todo +Testing".to_string()
            )
        );
    }

    #[test]
    fn todo_inside_double_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &DOCKERFILE);
        let text = r##"
FROM ubuntu:22.04
ENV MSG="# TODO this is inside a string"

# TODO this is a real todo
RUN apt-get update
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn todo_inside_single_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &DOCKERFILE);
        let text = r##"
FROM ubuntu:22.04
ENV MSG='# TODO this is inside a string'

# TODO this is a real todo
RUN apt-get update
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_double_quoted_string() {
        let parser = crate::lang::Parser::new("TODO", &DOCKERFILE);
        let text = r##"
ENV MSG="hello \"
# TODO false positive
world"

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escaped_single_quote_shell_idiom() {
        // Shell idiom in RUN commands: 'hello '\''world'
        let parser = crate::lang::Parser::new("TODO", &DOCKERFILE);
        let text = r##"
FROM ubuntu:22.04
RUN echo 'hello '\''
# TODO false positive
world'

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }
}
