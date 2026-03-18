use super::SyntaxRule;

pub const MARKDOWN: [SyntaxRule; 1] = [SyntaxRule::BlockComment("<!--", "-->")];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_todos() {
        let parser = crate::lang::Parser::new("TODO", &MARKDOWN);

        // Todo as block comment (end token on new line)
        let text = r#"
# My Document

<!-- TODO 2020-08-06 Can it handle block comments? +Testing

This is the description.
-->

Some markdown content here.
"#;
        assert_eq!(
            parser.parse_todos(text)[0],
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
# My Document

<!-- TODO 2020-08-06 Can it handle block comments? +Testing

This is the description. -->

Some markdown content here.
"#;
        assert_eq!(
            parser.parse_todos(text)[0],
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
# My Document

<!-- TODO 2020-08-06 Can it handle indented todos? +Testing

This is a test todo with some indented lines:
  - This is an even more indented line.
-->

Some markdown content here.
"#;
        assert_eq!(
            parser.parse_todos(text)[0],
            (
                4 as usize,
                8 as usize,
                r#"2020-08-06 Can it handle indented todos? +Testing

This is a test todo with some indented lines:
  - This is an even more indented line."#
                    .to_string()
            )
        );

        // Multiple todos
        let text = r#"
<!-- TODO 2020-08-06 First todo +Testing -->

# My Document

<!-- TODO 2020-08-06 Second todo +Testing
-->

Some content.
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
                6 as usize,
                7 as usize,
                "2020-08-06 Second todo +Testing".to_string()
            )
        );
    }
}
