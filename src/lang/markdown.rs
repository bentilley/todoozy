use super::SyntaxRule;

pub const MARKDOWN: [SyntaxRule; 3] = [
    SyntaxRule::BlockComment(b"<!--", b"-->"),
    SyntaxRule::SkipDelimited(b"```", b"```"),
    SyntaxRule::SkipDelimited(b"`", b"`"),
];

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

    #[test]
    fn todo_inside_inline_code_ignored() {
        let parser = crate::lang::Parser::new("TODO", &MARKDOWN);
        let text = r#"
# My Document

Here is some `<!-- TODO this is inside inline code -->` text.

<!-- TODO this is a real todo -->

More content.
"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn todo_inside_fenced_code_block_ignored() {
        let parser = crate::lang::Parser::new("TODO", &MARKDOWN);
        let text = r#"
# My Document

```
<!-- TODO this is inside a code block -->
```

<!-- TODO this is a real todo -->

More content.
"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn todo_inside_fenced_code_block_with_language_ignored() {
        let parser = crate::lang::Parser::new("TODO", &MARKDOWN);
        let text = r#"
# My Document

```rust
// TODO this is inside a code block
let x = 1;
```

<!-- TODO this is a real todo -->

More content.
"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }
}
