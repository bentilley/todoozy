use super::{Comment, SyntaxRule};

/// Skip `\'` outside of strings - this is a literal single quote, not a string start.
fn skip_escaped_single_quote<'a>(
    byte: u8,
    text: &'a [u8],
    pos: usize,
) -> Option<(usize, usize, Option<Comment<'a>>)> {
    if byte != b'\\' {
        return None;
    }
    if pos + 1 < text.len() && text[pos + 1] == b'\'' {
        Some((2, 0, None)) // skip 2 bytes, 0 newlines, no comment
    } else {
        None
    }
}

/// Skip here-docs: `<<EOF` ... `EOF`
/// Handles: <<EOF, <<'EOF', <<"EOF", <<-EOF (indented closing)
fn skip_heredoc<'a>(
    byte: u8,
    text: &'a [u8],
    pos: usize,
) -> Option<(usize, usize, Option<Comment<'a>>)> {
    if byte != b'<' {
        return None;
    }

    // Check for '<<' but not '<<<' (here-string)
    // Also make sure we're at the first '<', not in the middle of '<<<'
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

    // Skip optional '-' for <<-EOF (allows indented closing delimiter)
    let allow_indented_close = if cursor < text.len() && text[cursor] == b'-' {
        cursor += 1;
        true
    } else {
        false
    };

    // Skip whitespace before delimiter
    while cursor < text.len() && matches!(text[cursor], b' ' | b'\t') {
        cursor += 1;
    }

    // Parse delimiter (may be quoted)
    let delimiter = if cursor < text.len() && matches!(text[cursor], b'\'' | b'"') {
        let quote = text[cursor];
        cursor += 1;
        let start = cursor;
        while cursor < text.len() && text[cursor] != quote {
            cursor += 1;
        }
        let delim = &text[start..cursor];
        if cursor < text.len() {
            cursor += 1; // skip closing quote
        }
        delim
    } else {
        // Unquoted delimiter - read until whitespace/newline
        let start = cursor;
        while cursor < text.len() && !matches!(text[cursor], b' ' | b'\t' | b'\n' | b'\r') {
            cursor += 1;
        }
        &text[start..cursor]
    };

    if delimiter.is_empty() {
        return None;
    }

    // Skip to end of line (there may be more on this line after the delimiter declaration)
    while cursor < text.len() && text[cursor] != b'\n' {
        cursor += 1;
    }
    if cursor < text.len() {
        cursor += 1; // skip newline
    }

    let mut lines_seen = 1;

    // Scan line by line looking for closing delimiter
    while cursor < text.len() {
        let mut line_start = cursor;

        // For <<-, skip leading tabs
        if allow_indented_close {
            while line_start < text.len() && text[line_start] == b'\t' {
                line_start += 1;
            }
        }

        // Find end of line
        let mut line_end = line_start;
        while line_end < text.len() && text[line_end] != b'\n' {
            line_end += 1;
        }

        // Check if this line matches the delimiter exactly
        if &text[line_start..line_end] == delimiter {
            // Found closing delimiter - skip past it
            cursor = line_end;
            if cursor < text.len() {
                cursor += 1; // skip newline after delimiter
                lines_seen += 1;
            }
            return Some((cursor - pos, lines_seen, None));
        }

        // Move to next line
        cursor = line_end;
        if cursor < text.len() {
            cursor += 1;
            lines_seen += 1;
        }
    }

    // Reached EOF without finding closing delimiter - skip to end
    Some((cursor - pos, lines_seen, None))
}

pub const SH: [SyntaxRule; 5] = [
    SyntaxRule::LineComment(b"#"),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
    SyntaxRule::Custom(skip_escaped_single_quote), // must come before SkipDelimited for '
    SyntaxRule::SkipDelimited(b"'", b"'"),
    SyntaxRule::Custom(skip_heredoc),
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

    #[test]
    fn escaped_quote_in_double_quoted_string() {
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

    #[test]
    fn escaped_backslash_before_closing_double_quote() {
        // `\\` escapes the backslash, so the quote ends the string
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
msg="hello \\"

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn triple_backslash_before_double_quote() {
        // `\\\"` = escaped backslash + escaped quote, string continues
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
msg="hello \\\"
# TODO false positive
world"

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn multiple_escaped_double_quotes() {
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
msg="\"hello\" \"world\""

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escaped_double_quote_at_end_of_string() {
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
msg="hello \""

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn other_escape_sequences_in_double_quoted_string() {
        // Various escapes shouldn't break parsing
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
msg="hello\nworld\t\$var"

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escape_sequences_mixed_with_escaped_double_quotes() {
        // Mix of regular escapes and escaped quotes
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
msg="line1\nline2\t\"quoted\"\\done"

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_single_quoted_string() {
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

    #[test]
    fn heredoc() {
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

    #[test]
    fn heredoc_single_quoted_delimiter() {
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
cat <<'EOF'
# TODO inside here-doc
EOF

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn heredoc_double_quoted_delimiter() {
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
cat <<"EOF"
# TODO inside here-doc
EOF

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn heredoc_indented_close() {
        // <<- allows the closing delimiter to be indented with tabs
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = "
cat <<-EOF
\t# TODO inside here-doc
\tEOF

# TODO real todo
";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn heredoc_delimiter_in_content() {
        // EOF appearing as part of a line shouldn't close the here-doc
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
cat <<EOF
# TODO inside - this line mentions EOF but isn't the closing
some text EOF more text
EOF

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn heredoc_multiple() {
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
cat <<EOF
# TODO inside first here-doc
EOF

# TODO between here-docs

cat <<END
# TODO inside second here-doc
END

# TODO after here-docs
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].2, "between here-docs".to_string());
        assert_eq!(todos[1].2, "after here-docs".to_string());
    }

    #[test]
    fn heredoc_not_herestring() {
        // <<< is a here-string, not here-doc - should not be matched
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
cat <<< "hello"

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn herestring_with_quoted_todo_ignored() {
        // TODO inside a quoted here-string should be ignored
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
cat <<< "# TODO inside here-string"

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn herestring_unquoted_with_comment() {
        // `cat <<< word # comment` - the # starts a real comment
        let parser = crate::lang::Parser::new("TODO", &SH);
        let text = r##"
cat <<< hello # TODO this is a comment after here-string

# TODO second todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].2, "this is a comment after here-string".to_string());
        assert_eq!(todos[1].2, "second todo".to_string());
    }
}
