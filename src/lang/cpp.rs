use super::{Comment, SyntaxRule};

/// Skip C++ raw string literals: R"delimiter(content)delimiter"
/// The delimiter can be 0-16 characters (we allow any length here).
/// Returns (bytes_consumed, newlines_counted, None) to skip the literal.
pub fn skip_raw_string_literal<'a>(
    byte: u8,
    text: &'a [u8],
    pos: usize,
) -> Option<(usize, usize, Option<Comment<'a>>)> {
    // Must start with R"
    if byte != b'R' || pos + 1 >= text.len() || text[pos + 1] != b'"' {
        return None;
    }

    // Find opening paren - delimiter is between R" and (
    let after_quote = pos + 2;
    let mut paren_pos = after_quote;
    while paren_pos < text.len() && text[paren_pos] != b'(' {
        // Delimiter can only contain certain characters (not parentheses, backslash, or whitespace)
        let c = text[paren_pos];
        if c == b')' || c == b'\\' || c == b'"' || c.is_ascii_whitespace() {
            return None;
        }
        paren_pos += 1;
    }

    if paren_pos >= text.len() {
        return None;
    }

    let delimiter = &text[after_quote..paren_pos];
    let content_start = paren_pos + 1;

    // Build closing sequence: )delimiter"
    let mut closing = Vec::with_capacity(delimiter.len() + 2);
    closing.push(b')');
    closing.extend_from_slice(delimiter);
    closing.push(b'"');

    // Find the closing sequence
    let mut i = content_start;
    let mut newlines = 0;
    while i + closing.len() <= text.len() {
        if text[i..].starts_with(&closing) {
            let total_consumed = (i + closing.len()) - pos;
            return Some((total_consumed, newlines, None));
        }
        if text[i] == b'\n' {
            newlines += 1;
        }
        i += 1;
    }

    // Unterminated raw string - consume to end
    while i < text.len() {
        if text[i] == b'\n' {
            newlines += 1;
        }
        i += 1;
    }
    Some((text.len() - pos, newlines, None))
}

pub const CPP: [SyntaxRule; 5] = [
    SyntaxRule::LineComment(b"//"),
    SyntaxRule::BlockComment(b"/*", b"*/"),
    SyntaxRule::Custom(skip_raw_string_literal), // Must come before regular strings
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
    SyntaxRule::SkipDelimitedWithEscape(b"'", b"'", b'\\'),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    // All C tests should work for C++ as well

    #[test]
    fn line_comment_todo() {
        let parser = crate::lang::Parser::new("TODO", &CPP);
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
        let parser = crate::lang::Parser::new("TODO", &CPP);
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
        let parser = crate::lang::Parser::new("TODO", &CPP);
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
        let parser = crate::lang::Parser::new("TODO", &CPP);
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
    fn todo_inside_double_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &CPP);
        let text = r#"
std::string msg = "// TODO this should be ignored";

// TODO this should be found
int x = 1;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this should be found".to_string());
    }

    #[test]
    fn todo_inside_character_literal_ignored() {
        let parser = crate::lang::Parser::new("TODO", &CPP);
        let text = r#"
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
        let parser = crate::lang::Parser::new("TODO", &CPP);
        let text = r#"
std::string msg = "hello \"
// TODO false positive
world";

// TODO real todo
int x = 1;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    // C++ raw string literal tests

    #[test]
    fn raw_string_literal_no_delimiter() {
        let parser = crate::lang::Parser::new("TODO", &CPP);
        let text = r##"
std::string msg = R"(// TODO this should be ignored)";

// TODO this should be found
int x = 1;
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this should be found".to_string());
    }

    #[test]
    fn raw_string_literal_with_delimiter() {
        let parser = crate::lang::Parser::new("TODO", &CPP);
        let text = r##"
std::string msg = R"xyz(// TODO this should be ignored)xyz";

// TODO this should be found
int x = 1;
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this should be found".to_string());
    }

    #[test]
    fn raw_string_literal_with_embedded_quotes() {
        let parser = crate::lang::Parser::new("TODO", &CPP);
        let text = r##"
std::string msg = R"(contains "quotes" and // TODO fake)";

// TODO real todo
int x = 1;
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn raw_string_literal_multiline() {
        let parser = crate::lang::Parser::new("TODO", &CPP);
        let text = r##"
std::string msg = R"(
line 1
// TODO this is inside raw string
line 3
)";

// TODO real todo
int x = 1;
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn raw_string_literal_with_closing_paren() {
        // The ) alone doesn't close it - needs )delimiter"
        let parser = crate::lang::Parser::new("TODO", &CPP);
        let text = r##"
std::string msg = R"delim(
contains ) and )" and )delim without quote
// TODO fake
)delim";

// TODO real todo
int x = 1;
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn raw_string_after_regular_code() {
        let parser = crate::lang::Parser::new("TODO", &CPP);
        let text = r##"
int x = 1;
std::string msg = R"(raw content)";

// TODO after raw string
int y = 2;
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "after raw string".to_string());
    }

    #[test]
    fn multiple_raw_strings() {
        let parser = crate::lang::Parser::new("TODO", &CPP);
        let text = r##"
std::string a = R"(// TODO fake 1)";
std::string b = R"abc(// TODO fake 2)abc";

// TODO real todo
int x = 1;
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn inline_todo() {
        let parser = crate::lang::Parser::new("TODO", &CPP);
        let text = r#"int x = 1; // TODO fix this
int y = 2;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "fix this\n\n`int x = 1;`".to_string()));
    }

    #[test]
    fn r_not_followed_by_quote_is_not_raw_string() {
        // R without " immediately after is just a variable/identifier
        let parser = crate::lang::Parser::new("TODO", &CPP);
        let text = r#"
int R = 1;
// TODO real todo
int x = R + 1;
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }
}
