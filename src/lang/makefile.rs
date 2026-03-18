use super::SyntaxRule;

pub const MAKEFILE: [SyntaxRule; 1] = [SyntaxRule::LineComment(b"#")];

#[cfg(test)]
mod tests {
    use super::*;

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
}
