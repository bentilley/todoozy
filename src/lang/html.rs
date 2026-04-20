use super::SyntaxRule;

pub const HTML: [SyntaxRule; 1] = [
    SyntaxRule::BlockComment(b"<!--", b"-->"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    #[test]
    fn basic_todo_in_html_comment() {
        let parser = crate::lang::Parser::new("TODO", &HTML);
        let text = r#"<html>
<!-- TODO fix the layout -->
<body>Hello</body>
</html>"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (2, 2, "fix the layout".to_string()));
    }

    #[test]
    fn multiline_todo_in_html_comment() {
        let parser = crate::lang::Parser::new("TODO", &HTML);
        let text = r#"<html>
<!--
  TODO refactor this section

  This needs to be cleaned up.
-->
<body>Hello</body>
</html>"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                2,
                6,
                "refactor this section\n\nThis needs to be cleaned up.".to_string()
            )
        );
    }

    #[test]
    fn todo_inside_attribute_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &HTML);
        let text = r#"<html>
<div title="TODO this is not a real todo">content</div>
<!-- TODO this is a real todo -->
</html>"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn inline_todo() {
        let parser = crate::lang::Parser::new("TODO", &HTML);
        let text = r#"<p>Some text</p><!-- TODO inline comment --><p>More text</p>"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "inline comment".to_string()));
    }

    #[test]
    fn multiple_todos() {
        let parser = crate::lang::Parser::new("TODO", &HTML);
        let text = r#"<!-- TODO first todo -->
<div>content</div>
<!-- TODO second todo -->"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0], (1, 1, "first todo".to_string()));
        assert_eq!(todos[1], (3, 3, "second todo".to_string()));
    }

    #[test]
    fn todo_with_colon() {
        let parser = crate::lang::Parser::new("TODO", &HTML);
        let text = r#"<!-- TODO: with colon -->"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "with colon".to_string()));
    }
}
