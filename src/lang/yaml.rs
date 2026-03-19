use super::SyntaxRule;

pub const YAML: [SyntaxRule; 3] = [
    SyntaxRule::LineComment(b"#"),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
    SyntaxRule::SkipDelimited(b"'", b"'"), // YAML single quotes don't support escaping
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_todos() {
        let parser = crate::lang::Parser::new("TODO", &YAML);

        // Todo as line comments
        let text = r#"
name: example
version: 1.0.0

# TODO 2020-08-06 Can it handle line comments? +Testing
#
# This is the description.
services:
  web:
    image: nginx
"#;
        assert_eq!(
            parser.parse_todos(text)[0],
            (
                5 as usize,
                7 as usize,
                r#"2020-08-06 Can it handle line comments? +Testing

This is the description."#
                    .to_string()
            )
        );

        // Todo with indented lines
        let text = r#"
name: example
version: 1.0.0

# TODO 2020-08-06 Can it handle indented todos? +Testing
#
# This is a test todo with some indented lines:
#   - This is an even more indented line.

services:
  web:
    image: nginx
"#;
        assert_eq!(
            parser.parse_todos(text)[0],
            (
                5 as usize,
                8 as usize,
                r#"2020-08-06 Can it handle indented todos? +Testing

This is a test todo with some indented lines:
  - This is an even more indented line."#
                    .to_string()
            )
        );

        // Multiple todos
        let text = r#"
# TODO 2020-08-06 First todo +Testing
name: example

# TODO 2020-08-06 Second todo +Testing
version: 1.0.0
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
        let parser = crate::lang::Parser::new("TODO", &YAML);
        let text = r##"
name: example
message: "# TODO this is inside a string"

# TODO this is a real todo
version: 1.0.0
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn todo_inside_single_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &YAML);
        let text = r##"
name: example
message: '# TODO this is inside a string'

# TODO this is a real todo
version: 1.0.0
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_double_quoted_string() {
        let parser = crate::lang::Parser::new("TODO", &YAML);
        let text = r##"
message: "hello \"
# TODO false positive
world"

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }
}
