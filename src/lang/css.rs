use super::SyntaxRule;

pub const CSS: [SyntaxRule; 3] = [
    SyntaxRule::BlockComment(b"/*", b"*/"),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
    SyntaxRule::SkipDelimitedWithEscape(b"'", b"'", b'\\'),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    #[test]
    fn basic_todo_in_block_comment() {
        let parser = crate::lang::Parser::new("TODO", &CSS);
        let text = r#"/* TODO fix this selector */
.class { color: red; }"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "fix this selector".to_string()));
    }

    #[test]
    fn multiline_block_comment() {
        let parser = crate::lang::Parser::new("TODO", &CSS);
        let text = r#"/*
  TODO refactor these styles

  This section needs cleanup.
*/
.class { color: red; }"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                1,
                5,
                "refactor these styles\n\nThis section needs cleanup.".to_string()
            )
        );
    }

    #[test]
    fn todo_inside_double_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &CSS);
        let text = r#".class {
    content: "TODO this is not a real todo";
}
/* TODO this is a real todo */"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn todo_inside_single_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &CSS);
        let text = r#".class {
    content: 'TODO this is not a real todo';
}
/* TODO this is a real todo */"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_string() {
        let parser = crate::lang::Parser::new("TODO", &CSS);
        let text = r#".class {
    content: "escaped \" quote TODO not a todo";
}
/* TODO real todo */"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn multiple_todos() {
        let parser = crate::lang::Parser::new("TODO", &CSS);
        let text = r#"/* TODO first todo */
.class { color: red; }
/* TODO second todo */"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0], (1, 1, "first todo".to_string()));
        assert_eq!(todos[1], (3, 3, "second todo".to_string()));
    }

    #[test]
    fn todo_with_colon() {
        let parser = crate::lang::Parser::new("TODO", &CSS);
        let text = r#"/* TODO: with colon */"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "with colon".to_string()));
    }
}
