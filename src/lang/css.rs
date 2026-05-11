use super::SyntaxRule;

pub const CSS: [SyntaxRule; 3] = [
    SyntaxRule::BlockComment(b"/*", b"*/"),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
    SyntaxRule::SkipDelimitedWithEscape(b"'", b"'", b'\\'),
];

pub const SCSS: [SyntaxRule; 4] = [
    SyntaxRule::LineComment(b"//"),
    SyntaxRule::BlockComment(b"/*", b"*/"),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
    SyntaxRule::SkipDelimitedWithEscape(b"'", b"'", b'\\'),
];

pub const LESS: [SyntaxRule; 4] = [
    SyntaxRule::LineComment(b"//"),
    SyntaxRule::BlockComment(b"/*", b"*/"),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
    SyntaxRule::SkipDelimitedWithEscape(b"'", b"'", b'\\'),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    #[test]
    fn css_block_comment_todo() {
        let parser = crate::lang::Parser::new("TODO", &CSS);
        let text = r#"
.button {
    /* TODO 2020-08-06 Can it handle block comments? +Testing

       This is the description.
     */
    color: red;
}
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                3,
                6,
                r#"2020-08-06 Can it handle block comments? +Testing

This is the description."#
                    .to_string()
            )
        );
    }

    #[test]
    fn css_single_line_block_comment_todo() {
        let parser = crate::lang::Parser::new("TODO", &CSS);
        let text = r#"
.button { /* TODO fix button spacing */ }
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (2, 2, "fix button spacing".to_string()));
    }

    #[test]
    fn css_todo_inside_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &CSS);
        let text = r#"
.button::before {
    content: "/* TODO this should be ignored */";
}

/* TODO this is a real todo */
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn scss_line_comment_todo() {
        let parser = crate::lang::Parser::new("TODO", &SCSS);
        let text = r#"
.button {
    // TODO 2020-08-06 Can it handle line comments? +Testing
    //
    // This is the description.
    color: red;
}
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                3,
                5,
                r#"2020-08-06 Can it handle line comments? +Testing

This is the description."#
                    .to_string()
            )
        );
    }

    #[test]
    fn scss_block_comment_todo() {
        let parser = crate::lang::Parser::new("TODO", &SCSS);
        let text = r#"
.button {
    /* TODO 2020-08-06 Can it handle block comments? +Testing

       This is the description.
     */
    color: red;
}
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                3,
                6,
                r#"2020-08-06 Can it handle block comments? +Testing

This is the description."#
                    .to_string()
            )
        );
    }

    #[test]
    fn scss_inline_todo() {
        let parser = crate::lang::Parser::new("TODO", &SCSS);
        let text = r#"
.button { color: red; } // TODO move color to token
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                2,
                2,
                "move color to token\n\n`.button { color: red; }`".to_string()
            )
        );
    }

    #[test]
    fn scss_todo_inside_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &SCSS);
        let text = r#"
$url: "http://example.com//TODO-not-a-comment";
$text: '/* TODO this should be ignored */';

// TODO this is a real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn less_line_comment_todo() {
        let parser = crate::lang::Parser::new("TODO", &LESS);
        let text = r#"
.button {
    // TODO convert hard-coded color to variable
    color: red;
}
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (3, 3, "convert hard-coded color to variable".to_string())
        );
    }

    #[test]
    fn less_block_comment_todo() {
        let parser = crate::lang::Parser::new("TODO", &LESS);
        let text = r#"
/* TODO support theme variants

   This needs shared variables.
 */
.button {
    color: red;
}
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                2,
                5,
                "support theme variants\n\nThis needs shared variables.".to_string()
            )
        );
    }

    #[test]
    fn less_todo_inside_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &LESS);
        let text = r#"
@value: "// TODO ignore this";

// TODO keep this
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "keep this".to_string());
    }
}
