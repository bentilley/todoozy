use super::{Comment, SyntaxRule};

/// Skip PHP heredoc and nowdoc: `<<<LABEL` / `<<<'LABEL'` ... `LABEL` (on its own line).
fn skip_heredoc<'a>(
    byte: u8,
    text: &'a [u8],
    pos: usize,
) -> Option<(usize, usize, Option<Comment<'a>>)> {
    if byte != b'<' {
        return None;
    }

    // Must start with '<<<'
    if pos + 2 >= text.len() || text[pos + 1] != b'<' || text[pos + 2] != b'<' {
        return None;
    }

    let mut cursor = pos + 3;

    // Skip optional whitespace (PHP 7.3+ allows space after <<<)
    while cursor < text.len() && matches!(text[cursor], b' ' | b'\t') {
        cursor += 1;
    }

    // Parse delimiter — may be quoted with ' (nowdoc) or " (heredoc with interpolation)
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
        let start = cursor;
        while cursor < text.len() && !matches!(text[cursor], b' ' | b'\t' | b'\n' | b'\r') {
            cursor += 1;
        }
        &text[start..cursor]
    };

    if delimiter.is_empty() {
        return None;
    }

    // Skip to end of opening line
    while cursor < text.len() && text[cursor] != b'\n' {
        cursor += 1;
    }
    if cursor < text.len() {
        cursor += 1; // skip newline
    }

    let mut lines_seen = 1;

    // Scan line-by-line for the closing delimiter
    while cursor < text.len() {
        // PHP 7.3+: closing delimiter may be indented
        let mut line_start = cursor;
        while line_start < text.len() && matches!(text[line_start], b' ' | b'\t') {
            line_start += 1;
        }

        // Find end of line
        let mut line_end = line_start;
        while line_end < text.len() && text[line_end] != b'\n' {
            line_end += 1;
        }

        // The closing line starts with the delimiter, optionally followed by ';' and/or whitespace
        let line = &text[line_start..line_end];
        if line.starts_with(delimiter) {
            let rest = &line[delimiter.len()..];
            let rest_trimmed = rest.iter().position(|&b| b != b';' && b != b' ' && b != b'\t');
            if rest_trimmed.is_none() {
                // Entire rest is `;` and/or whitespace — this is the closing delimiter
                cursor = line_end;
                if cursor < text.len() {
                    cursor += 1; // skip newline
                    lines_seen += 1;
                }
                return Some((cursor - pos, lines_seen, None));
            }
        }

        cursor = line_end;
        if cursor < text.len() {
            cursor += 1;
            lines_seen += 1;
        }
    }

    // EOF without closing delimiter
    Some((cursor - pos, lines_seen, None))
}

pub const PHP: [SyntaxRule; 6] = [
    SyntaxRule::LineComment(b"//"),
    SyntaxRule::LineComment(b"#"),
    SyntaxRule::BlockComment(b"/*", b"*/"),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
    SyntaxRule::SkipDelimitedWithEscape(b"'", b"'", b'\\'),
    SyntaxRule::Custom(skip_heredoc),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    #[test]
    fn test_parser() {
        let parser = crate::lang::Parser::new("TODO", &PHP);

        // Todo as // line comment
        let text = r#"
<?php
$x = 1;

// TODO 2020-08-06 Can it handle // line comments? +Testing
//
// This is the description.
$y = 2;
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                5,
                7,
                "2020-08-06 Can it handle // line comments? +Testing\n\nThis is the description."
                    .to_string()
            )
        );

        // Todo as # line comment
        let text = r#"
<?php
$x = 1;

# TODO 2020-08-06 Can it handle # line comments? +Testing
#
# This is the description.
$y = 2;
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                5,
                7,
                "2020-08-06 Can it handle # line comments? +Testing\n\nThis is the description."
                    .to_string()
            )
        );

        // Todo as block comment
        let text = r#"
<?php
$x = 1;

/* TODO 2020-08-06 Can it handle block comments? +Testing

   This is the description.
 */
$y = 2;
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                5,
                8,
                "2020-08-06 Can it handle block comments? +Testing\n\nThis is the description."
                    .to_string()
            )
        );

        // Todo inside double-quoted string should be ignored
        let text = r#"
<?php
$msg = "// TODO this is inside a string";

// TODO this is a real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());

        // Todo inside single-quoted string should be ignored
        let text = r#"
<?php
$msg = '// TODO this is inside a string';

// TODO this is a real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());

        // Todo inside heredoc should be ignored
        let text = r#"
<?php
$text = <<<EOT
// TODO this is inside a heredoc
EOT;

// TODO this is a real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());

        // Todo inside nowdoc should be ignored
        let text = r#"
<?php
$text = <<<'EOT'
// TODO this is inside a nowdoc
EOT;

// TODO this is a real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn heredoc_closing_with_semicolon() {
        let parser = crate::lang::Parser::new("TODO", &PHP);
        let text = r#"
<?php
$text = <<<EOT
// TODO false positive
EOT;

// TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn double_quoted_heredoc_skipped() {
        let parser = crate::lang::Parser::new("TODO", &PHP);
        let text = r#"
<?php
$text = <<<"EOT"
// TODO false positive
EOT;

// TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn todo_inside_block_comment_after_heredoc() {
        let parser = crate::lang::Parser::new("TODO", &PHP);
        let text = r#"
<?php
$text = <<<EOT
nothing here
EOT;

/* TODO after heredoc */
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "after heredoc".to_string());
    }

    #[test]
    fn escaped_quote_in_double_quoted_string() {
        let parser = crate::lang::Parser::new("TODO", &PHP);
        let text = r#"
$msg = "hello \"
// TODO false positive
world";

// TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_single_quoted_string() {
        let parser = crate::lang::Parser::new("TODO", &PHP);
        let text = r#"
$msg = 'hello \'
// TODO false positive
world';

// TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }
}
