use super::{Comment, SyntaxRule};

/// Skip C# verbatim string literals such as `@"..."` and interpolated verbatim
/// strings such as `@$"..."`, where doubled quotes `""` escape a quote.
fn skip_verbatim_string_literal<'a>(
    byte: u8,
    text: &'a [u8],
    pos: usize,
) -> Option<(usize, usize, Option<Comment<'a>>)> {
    if byte != b'@' {
        return None;
    }

    let mut cursor = pos + 1;
    while cursor < text.len() && text[cursor] == b'$' {
        cursor += 1;
    }

    if cursor >= text.len() || text[cursor] != b'"' {
        return None;
    }

    cursor += 1;
    let mut newlines = 0;

    while cursor < text.len() {
        if text[cursor] == b'\n' {
            newlines += 1;
            cursor += 1;
            continue;
        }

        if text[cursor] == b'"' {
            if cursor + 1 < text.len() && text[cursor + 1] == b'"' {
                cursor += 2;
                continue;
            }

            return Some((cursor + 1 - pos, newlines, None));
        }

        cursor += 1;
    }

    Some((text.len() - pos, newlines, None))
}

/// Skip C# raw string literals that start with three or more quotes, e.g.
/// `"""..."""` or `""""...""""`.
fn skip_raw_string_literal<'a>(
    byte: u8,
    text: &'a [u8],
    pos: usize,
) -> Option<(usize, usize, Option<Comment<'a>>)> {
    if byte != b'"' {
        return None;
    }

    let mut quote_count = 0;
    while pos + quote_count < text.len() && text[pos + quote_count] == b'"' {
        quote_count += 1;
    }

    if quote_count < 3 {
        return None;
    }

    let mut cursor = pos + quote_count;
    let mut newlines = 0;

    while cursor < text.len() {
        if text[cursor] == b'\n' {
            newlines += 1;
        }

        if cursor + quote_count <= text.len()
            && text[cursor..cursor + quote_count].iter().all(|&b| b == b'"')
        {
            return Some((cursor + quote_count - pos, newlines, None));
        }

        cursor += 1;
    }

    Some((text.len() - pos, newlines, None))
}

pub const CSHARP: [SyntaxRule; 6] = [
    SyntaxRule::LineComment(b"//"),
    SyntaxRule::BlockComment(b"/*", b"*/"),
    SyntaxRule::Custom(skip_verbatim_string_literal), // @"..." and @$"..."
    SyntaxRule::Custom(skip_raw_string_literal),      // """...""" and longer quote counts
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
    SyntaxRule::SkipDelimitedWithEscape(b"'", b"'", b'\\'),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    #[test]
    fn line_comment_todo() {
        let parser = crate::lang::Parser::new("TODO", &CSHARP);
        let text = r#"
class Program {
    // TODO fix this method
}
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "fix this method".to_string());
    }

    #[test]
    fn line_comment_multiline_todo() {
        let parser = crate::lang::Parser::new("TODO", &CSHARP);
        let text = r#"
class Program {
    // TODO refactor this code
    // to use a better algorithm
    // for performance
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
        let parser = crate::lang::Parser::new("TODO", &CSHARP);
        let text = r#"
class Program {
    /* TODO add error handling */
}
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "add error handling".to_string());
    }

    #[test]
    fn block_comment_multiline_todo() {
        let parser = crate::lang::Parser::new("TODO", &CSHARP);
        let text = r#"
class Program {
    /* TODO implement feature

       This needs to handle:
       - Case A
       - Case B
     */
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
    fn todo_inside_regular_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &CSHARP);
        let text = r#"
var msg = "// TODO this should be ignored";

// TODO this should be found
var x = 1;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this should be found".to_string());
    }

    #[test]
    fn todo_inside_character_literal_ignored() {
        let parser = crate::lang::Parser::new("TODO", &CSHARP);
        let text = r#"
char slash = '/';
char quote = '\'';

// TODO real todo
var x = 1;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_string() {
        let parser = crate::lang::Parser::new("TODO", &CSHARP);
        let text = r#"
var msg = "hello \"
// TODO false positive
world";

// TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn inline_todo() {
        let parser = crate::lang::Parser::new("TODO", &CSHARP);
        let text = r#"var x = 1; // TODO fix this
var y = 2;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "fix this\n\n`var x = 1;`".to_string()));
    }

    #[test]
    fn multiple_todos() {
        let parser = crate::lang::Parser::new("TODO", &CSHARP);
        let text = r#"
// TODO first todo
var x = 1;

/* TODO second todo */
var y = 2;

// TODO third todo
var z = 3;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 3);
        assert_eq!(todos[0].2, "first todo".to_string());
        assert_eq!(todos[1].2, "second todo".to_string());
        assert_eq!(todos[2].2, "third todo".to_string());
    }

    #[test]
    fn todo_inside_verbatim_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &CSHARP);
        let text = r#"
var text = @"line 1
// TODO false positive
line ""quoted""";

// TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn todo_inside_interpolated_verbatim_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &CSHARP);
        let text = r#"
var text = @$"line 1
// TODO false positive
line ""quoted""";

// TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn todo_inside_raw_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &CSHARP);
        let text = r#"
var json = """
{
  // TODO false positive
}
""";

// TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn todo_inside_longer_raw_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &CSHARP);
        let text = r#"
var value = """"
text with """
// TODO false positive
"""";

// TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }
}
