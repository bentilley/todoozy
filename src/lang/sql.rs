use super::SyntaxRule;

pub const SQL: [SyntaxRule; 4] = [
    SyntaxRule::LineComment(b"--"),
    SyntaxRule::BlockComment(b"/*", b"*/"),
    SyntaxRule::SkipDelimited(b"'", b"'"),
    SyntaxRule::SkipDelimited(b"\"", b"\""),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    #[test]
    fn test_parse_str() {
        let parser = crate::lang::Parser::new("TODO", &SQL);

        // Todo as line comments
        let text = r#"
    SELECT 1;

    -- TODO 2020-08-06 Can it handle line comments? +Testing
    --
    -- This is the description.
    SELECT 2;
"#;
        assert_eq!(
            parser.parse_str(text)[0],
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
    SELECT 1;

    /* TODO 2020-08-06 Can it handle block comments? +Testing

       This is the description.
     */
    SELECT 2;
"#;
        assert_eq!(
            parser.parse_str(text)[0],
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
    SELECT 1;

    /* TODO 2020-08-06 Can it handle block comments? +Testing

       This is the description. */
    SELECT 2;
"#;
        assert_eq!(
            parser.parse_str(text)[0],
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
    SELECT 1;

    /* TODO 2020-08-06 Can it handle indented todos? +Testing

       This is a test todo with some indented lines:
         - This is an even more indented line.
     */

    SELECT 2;
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                4 as usize,
                8 as usize,
                r#"2020-08-06 Can it handle indented todos? +Testing

This is a test todo with some indented lines:
  - This is an even more indented line."#
                    .to_string()
            )
        );
    }

    #[test]
    fn todo_inside_single_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &SQL);
        let text = r#"
SELECT '-- TODO this is inside a string';

-- TODO this is a real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn todo_inside_double_quoted_identifier_ignored() {
        let parser = crate::lang::Parser::new("TODO", &SQL);
        let text = r#"
SELECT 1 AS "-- TODO this is inside an identifier";

-- TODO this is a real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn doubled_single_quote_keeps_string_open() {
        let parser = crate::lang::Parser::new("TODO", &SQL);
        let text = r#"
SELECT 'hello ''
-- TODO false positive
world';

-- TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn doubled_double_quote_keeps_identifier_open() {
        let parser = crate::lang::Parser::new("TODO", &SQL);
        let text = r#"
SELECT 1 AS "hello ""
-- TODO false positive
world";

-- TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }
}
