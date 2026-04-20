use super::SyntaxRule;

pub const C: [SyntaxRule; 4] = [
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
    fn line_comment_todo() {
        let parser = crate::lang::Parser::new("TODO", &C);
        let text = r#"
int main() {
    // TODO fix this function
    return 0;
}
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "fix this function".to_string());
    }

    #[test]
    fn line_comment_multiline_todo() {
        let parser = crate::lang::Parser::new("TODO", &C);
        let text = r#"
int main() {
    // TODO refactor this code
    // to use a better algorithm
    // for performance
    return 0;
}
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                3,
                5,
                "refactor this code\nto use a better algorithm\nfor performance".to_string()
            )
        );
    }

    #[test]
    fn block_comment_todo() {
        let parser = crate::lang::Parser::new("TODO", &C);
        let text = r#"
int main() {
    /* TODO add error handling */
    return 0;
}
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "add error handling".to_string());
    }

    #[test]
    fn block_comment_multiline_todo() {
        let parser = crate::lang::Parser::new("TODO", &C);
        let text = r#"
int main() {
    /* TODO implement feature

       This needs to handle:
       - Case A
       - Case B
     */
    return 0;
}
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                3,
                8,
                "implement feature\n\nThis needs to handle:\n- Case A\n- Case B".to_string()
            )
        );
    }

    #[test]
    fn nested_block_comment() {
        let parser = crate::lang::Parser::new("TODO", &C);
        let text = r#"
/* TODO outer /* inner comment */ still outer */
int x = 1;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "outer /* inner comment */ still outer".to_string());
    }

    #[test]
    fn todo_inside_double_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &C);
        let text = r#"
char *msg = "// TODO this should be ignored";

// TODO this should be found
int x = 1;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this should be found".to_string());
    }

    #[test]
    fn todo_inside_character_literal_ignored() {
        let parser = crate::lang::Parser::new("TODO", &C);
        let text = r#"
// Note: multi-char literals are technically valid in C (implementation-defined)
int x = 'TODO';

// TODO real todo
int y = 1;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_string() {
        let parser = crate::lang::Parser::new("TODO", &C);
        let text = r#"
char *msg = "hello \"
// TODO false positive
world";

// TODO real todo
int x = 1;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_char_literal() {
        let parser = crate::lang::Parser::new("TODO", &C);
        let text = r#"
char c = '\'';

// TODO real todo
int x = 1;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escaped_backslash_before_quote() {
        let parser = crate::lang::Parser::new("TODO", &C);
        let text = r#"
char *msg = "path\\";

// TODO real todo
int x = 1;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn inline_todo() {
        let parser = crate::lang::Parser::new("TODO", &C);
        let text = r#"int x = 1; // TODO fix this
int y = 2;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "fix this\n\n`int x = 1;`".to_string()));
    }

    #[test]
    fn multiple_todos() {
        let parser = crate::lang::Parser::new("TODO", &C);
        let text = r#"
// TODO first todo
int x = 1;

/* TODO second todo */
int y = 2;

// TODO third todo
int z = 3;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 3);
        assert_eq!(todos[0].2, "first todo".to_string());
        assert_eq!(todos[1].2, "second todo".to_string());
        assert_eq!(todos[2].2, "third todo".to_string());
    }
}
