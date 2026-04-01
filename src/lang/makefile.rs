use super::sh::skip_escaped_single_quote;
use super::SyntaxRule;

pub const MAKEFILE: [SyntaxRule; 4] = [
    SyntaxRule::LineComment(b"#"),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
    SyntaxRule::Custom(skip_escaped_single_quote), // must come before SkipDelimited for '
    SyntaxRule::SkipDelimited(b"'", b"'"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    #[test]
    fn test_parse_todos() {
        let parser = crate::lang::Parser::new("TODO", &MAKEFILE);

        // Todo as line comments
        let text = r#"
.PHONY: build test

# TODO 2020-08-06 Can it handle line comments? +Testing
#
# This is the description.
build:
	go build -o bin/app
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

        // Todo with indented lines
        let text = r#"
.PHONY: build test

# TODO 2020-08-06 Can it handle indented todos? +Testing
#
# This is a test todo with some indented lines:
#   - This is an even more indented line.

build:
	go build -o bin/app
"#;
        assert_eq!(
            parser.parse_todos(text)[0],
            (
                4 as usize,
                7 as usize,
                r#"2020-08-06 Can it handle indented todos? +Testing

This is a test todo with some indented lines:
  - This is an even more indented line."#
                    .to_string()
            )
        );

        // Multiple todos
        let text = r#"
# TODO 2020-08-06 First todo +Testing
.PHONY: build test

# TODO 2020-08-06 Second todo +Testing
build:
	go build -o bin/app
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
                5 as usize,
                5 as usize,
                "2020-08-06 Second todo +Testing".to_string()
            )
        );
    }

    // Shell string tests for recipe lines

    #[test]
    fn todo_inside_double_quoted_string_in_recipe() {
        let parser = crate::lang::Parser::new("TODO", &MAKEFILE);
        let text = r##"
.PHONY: build
build:
	echo "# TODO this is inside a string"

# TODO this is a real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn todo_inside_single_quoted_string_in_recipe() {
        let parser = crate::lang::Parser::new("TODO", &MAKEFILE);
        let text = r##"
.PHONY: build
build:
	echo '# TODO this is inside a string'

# TODO this is a real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_double_quoted_string() {
        let parser = crate::lang::Parser::new("TODO", &MAKEFILE);
        let text = r##"
.PHONY: build
build:
	echo "hello \"
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
        let parser = crate::lang::Parser::new("TODO", &MAKEFILE);
        let text = r##"
.PHONY: build
build:
	echo "hello \\"

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn triple_backslash_before_double_quote() {
        // `\\\"` = escaped backslash + escaped quote, string continues
        let parser = crate::lang::Parser::new("TODO", &MAKEFILE);
        let text = r##"
.PHONY: build
build:
	echo "hello \\\"
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
        let parser = crate::lang::Parser::new("TODO", &MAKEFILE);
        let text = r##"
.PHONY: build
build:
	echo "\"hello\" \"world\""

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escaped_double_quote_at_end_of_string() {
        let parser = crate::lang::Parser::new("TODO", &MAKEFILE);
        let text = r##"
.PHONY: build
build:
	echo "hello \""

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn other_escape_sequences_in_double_quoted_string() {
        let parser = crate::lang::Parser::new("TODO", &MAKEFILE);
        let text = r##"
.PHONY: build
build:
	echo "hello\nworld\t$$var"

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escape_sequences_mixed_with_escaped_double_quotes() {
        let parser = crate::lang::Parser::new("TODO", &MAKEFILE);
        let text = r##"
.PHONY: build
build:
	echo "line1\nline2\t\"quoted\"\\done"

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn escaped_single_quote_shell_idiom() {
        // Shell idiom: 'hello '\''world' = end string, escaped literal quote, new string
        let parser = crate::lang::Parser::new("TODO", &MAKEFILE);
        let text = r##"
.PHONY: build
build:
	echo 'hello '\''
# TODO false positive
world'

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn multiple_escaped_single_quotes_shell_idiom() {
        let parser = crate::lang::Parser::new("TODO", &MAKEFILE);
        let text = r##"
.PHONY: build
build:
	echo 'it'\''s a '\''test'\'''

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }

    #[test]
    fn mixed_quote_styles_in_recipe() {
        let parser = crate::lang::Parser::new("TODO", &MAKEFILE);
        let text = r##"
.PHONY: build
build:
	echo "double # not comment" 'single # not comment'

# TODO real todo
"##;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }
}
