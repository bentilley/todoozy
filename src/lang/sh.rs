use super::SyntaxRule;

pub const SH: [SyntaxRule; 3] = [
    SyntaxRule::LineComment(b"#"),
    SyntaxRule::SkipDelimited(b"\"", b"\""),
    SyntaxRule::SkipDelimited(b"'", b"'"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_todos() {
        let parser = crate::lang::Parser::new("TODO", &SH);

        // Todo as line comment
        let text = r#"
some="code"

# TODO 2020-08-06 Can it handle line comments? +Testing
#
# This is the description.
more="code"
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

        // Todo inside double-quoted string should be ignored
        let text = r##"
some="code"
msg="
# TODO this is inside a string
"

# TODO this is a real todo
more="code"
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (7, 7, "this is a real todo".to_string()));

        // Todo inside single-quoted string should be ignored
        let text = r##"
some='code'
msg='
# TODO this is inside a single-quoted string
'

# TODO this is a real todo
more='code'
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (7, 7, "this is a real todo".to_string()));
    }

    // TODO #46 (A) 2026-03-16 Fix " string parsing in shell lang
    #[test]
    #[ignore = "documents parsing limitation: escaped quotes"]
    fn escaped_quote_in_double_quoted_string_not_handled() {
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
msg="hello \"
# TODO false positive
world"

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    // TODO #47 (A) 2026-03-16 Fix ' string parsing in shell lang
    #[test]
    #[ignore = "documents parsing limitation: escaped quotes"]
    fn escaped_quote_in_single_quoted_string_not_handled() {
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
msg='hello '\''
# TODO false positive
world'

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    // TODO #48 (A) 2026-03-16 Fix here-doc parsing in shell lang
    #[test]
    #[ignore = "documents parsing limitation: here-docs not supported"]
    fn heredoc_not_handled() {
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
cat <<EOF
# TODO this is inside a here-doc
EOF

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }
}
