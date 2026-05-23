use super::{Comment, SyntaxRule};

pub const RUBY: [SyntaxRule; 5] = [
    SyntaxRule::LineComment(b"#"),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
    SyntaxRule::SkipDelimitedWithEscape(b"'", b"'", b'\\'),
    SyntaxRule::SkipDelimitedWithEscape(b"`", b"`", b'\\'),
    SyntaxRule::Custom(skip_ruby_heredoc),
];

fn skip_ruby_heredoc<'a>(
    byte: u8,
    text: &'a [u8],
    pos: usize,
) -> Option<(usize, usize, Option<Comment<'a>>)> {
    if byte != b'<' {
        return None;
    }
    if pos + 1 >= text.len() || text[pos + 1] != b'<' {
        return None;
    }
    if pos + 2 < text.len() && text[pos + 2] == b'<' {
        return None;
    }
    if pos > 0 && text[pos - 1] == b'<' {
        return None;
    }

    let mut cursor = pos + 2;

    let squiggly = cursor < text.len() && text[cursor] == b'~';
    let allow_indented_close = if cursor < text.len() && matches!(text[cursor], b'-' | b'~') {
        cursor += 1;
        true
    } else {
        false
    };

    while cursor < text.len() && matches!(text[cursor], b' ' | b'\t') {
        cursor += 1;
    }

    let delimiter = if cursor < text.len() && matches!(text[cursor], b'\'' | b'"') {
        let quote = text[cursor];
        cursor += 1;
        let start = cursor;
        while cursor < text.len() && text[cursor] != quote {
            cursor += 1;
        }
        let delim = &text[start..cursor];
        if cursor < text.len() {
            cursor += 1;
        }
        delim
    } else {
        let start = cursor;
        while cursor < text.len() && !matches!(text[cursor], b' ' | b'\t' | b'\n' | b'\r') {
            cursor += 1;
        }
        &text[start..cursor]
    };

    if delimiter.is_empty() {
        return None;
    }

    while cursor < text.len() && text[cursor] != b'\n' {
        cursor += 1;
    }
    if cursor < text.len() {
        cursor += 1;
    }

    let mut lines_seen = 1;

    while cursor < text.len() {
        let mut line_start = cursor;
        if allow_indented_close {
            if squiggly {
                while line_start < text.len() && matches!(text[line_start], b' ' | b'\t') {
                    line_start += 1;
                }
            } else {
                while line_start < text.len() && text[line_start] == b'\t' {
                    line_start += 1;
                }
            }
        }
        let mut line_end = line_start;
        while line_end < text.len() && text[line_end] != b'\n' {
            line_end += 1;
        }
        if &text[line_start..line_end] == delimiter {
            cursor = line_end;
            if cursor < text.len() {
                cursor += 1;
                lines_seen += 1;
            }
            return Some((cursor - pos, lines_seen, None));
        }
        cursor = line_end;
        if cursor < text.len() {
            cursor += 1;
            lines_seen += 1;
        }
    }

    Some((cursor - pos, lines_seen, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    #[test]
    fn line_comment_basic() {
        let parser = crate::lang::Parser::new("TODO", &RUBY);
        let text = r#"
x = 1

# TODO real todo
y = 2
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn todo_in_double_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &RUBY);
        let text = r##"
msg = "# TODO this is inside a string"

# TODO real todo
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn todo_in_single_quoted_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &RUBY);
        let text = r##"
msg = '# TODO this is inside a string'

# TODO real todo
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn todo_in_backtick_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &RUBY);
        let text = "
result = `# TODO this is inside a backtick string`

# TODO real todo
";
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_single_quoted_string() {
        let parser = crate::lang::Parser::new("TODO", &RUBY);
        let text = r##"
msg = 'it\'s fine'

# TODO real todo
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn heredoc_basic() {
        let parser = crate::lang::Parser::new("TODO", &RUBY);
        let text = r##"
text = <<HEREDOC
# TODO this is inside a heredoc
HEREDOC

# TODO real todo
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn heredoc_single_quoted_delimiter() {
        let parser = crate::lang::Parser::new("TODO", &RUBY);
        let text = r##"
text = <<'HEREDOC'
# TODO this is inside a heredoc
HEREDOC

# TODO real todo
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn heredoc_double_quoted_delimiter() {
        let parser = crate::lang::Parser::new("TODO", &RUBY);
        let text = r##"
text = <<"HEREDOC"
# TODO this is inside a heredoc
HEREDOC

# TODO real todo
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn heredoc_dash_indented() {
        let parser = crate::lang::Parser::new("TODO", &RUBY);
        let text = "
text = <<-HEREDOC
  # TODO this is inside a heredoc
\tHEREDOC

# TODO real todo
";
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn heredoc_squiggly() {
        let parser = crate::lang::Parser::new("TODO", &RUBY);
        let text = "
text = <<~HEREDOC
  # TODO this is inside a heredoc
  HEREDOC

# TODO real todo
";
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn heredoc_multiple() {
        let parser = crate::lang::Parser::new("TODO", &RUBY);
        let text = r##"
a = <<FIRST
# TODO inside first
FIRST

# TODO between heredocs

b = <<SECOND
# TODO inside second
SECOND

# TODO after heredocs
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].2, "between heredocs".to_string());
        assert_eq!(todos[1].2, "after heredocs".to_string());
    }
}
