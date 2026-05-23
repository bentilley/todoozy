use super::SyntaxRule;

pub const KOTLIN: [SyntaxRule; 4] = [
    SyntaxRule::LineComment(b"//"),
    SyntaxRule::BlockComment(b"/*", b"*/"),
    SyntaxRule::SkipDelimited(b"\"\"\"", b"\"\"\""),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    #[test]
    fn test_parser() {
        let parser = crate::lang::Parser::new("TODO", &KOTLIN);

        let text = r#"
val some = "code"

// TODO 2020-08-06 Can it handle line comments? +Testing
//
// This is the description.
val more = "code"
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                4,
                6,
                r#"2020-08-06 Can it handle line comments? +Testing

This is the description."#
                    .to_string()
            )
        );

        let text = r#"
val some = "code"

/* TODO 2020-08-06 Can it handle block comments? +Testing

   This is the description.
 */
val more = "code"
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                4,
                7,
                r#"2020-08-06 Can it handle block comments? +Testing

This is the description."#
                    .to_string()
            )
        );

        let text = r#"
val some = "code"

/* TODO 2020-08-06 Can it handle indented todos? +Testing

   This is a test todo with some indented lines:
     - This is an even more indented line.
 */

val more = "code"
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                4,
                8,
                r#"2020-08-06 Can it handle indented todos? +Testing

This is a test todo with some indented lines:
  - This is an even more indented line."#
                    .to_string()
            )
        );

        let text = r####"
val some = "code"
val text = """
    // TODO 2020-08-06 Can it handle this fake todo? +Testing
    //
    // This todo is in a triple-quoted string, so ignore it.
"""

// TODO 2020-08-06 Does it find the real todo? +Testing
//
// This todo isn't in a triple-quoted string.

val more = "code"
"####;
        assert_eq!(parser.parse_str(text).len(), 1);
        assert_eq!(
            parser.parse_str(text)[0],
            (
                9,
                11,
                r#"2020-08-06 Does it find the real todo? +Testing

This todo isn't in a triple-quoted string."#
                    .to_string()
            )
        );
    }

    #[test]
    fn nested_block_comment() {
        let parser = crate::lang::Parser::new("TODO", &KOTLIN);
        let text = r#"
/* TODO outer /* inner comment */ still outer */
val x = 1
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "outer /* inner comment */ still outer".to_string());
    }

    #[test]
    fn todo_inside_regular_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &KOTLIN);
        let text = r#"
val some = "code"
val msg = "// TODO this is inside a string"

// TODO this is a real todo
val more = "code"
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn todo_inside_triple_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &KOTLIN);
        let text = r####"
val some = "code"
val msg = """
// TODO this is inside a triple-quoted string
"""

// TODO this is a real todo
val more = "code"
"####;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_regular_string() {
        let parser = crate::lang::Parser::new("TODO", &KOTLIN);
        let text = r#"
val msg = "hello \"
// TODO false positive
world"

// TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }
}
