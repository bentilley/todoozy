use super::SyntaxRule;

pub const TERRAFORM: [SyntaxRule; 4] = [
    SyntaxRule::LineComment(b"#"),
    SyntaxRule::LineComment(b"//"),
    SyntaxRule::BlockComment(b"/*", b"*/"),
    SyntaxRule::SkipDelimitedWithEscape(b"\"", b"\"", b'\\'),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    #[test]
    fn test_parse_str() {
        let parser = crate::lang::Parser::new("TODO", &TERRAFORM);

        // Todo as hash line comments
        let text = r#"
    variable "example" {
      default = "value"
    }

    # TODO 2020-08-06 Can it handle hash line comments? +Testing
    #
    # This is the description.
    resource "aws_instance" "example" {
      ami = "ami-12345"
    }
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                6 as usize,
                8 as usize,
                r#"2020-08-06 Can it handle hash line comments? +Testing

This is the description."#
                    .to_string()
            )
        );

        // Todo as slash line comments
        let text = r#"
    variable "example" {
      default = "value"
    }

    // TODO 2020-08-06 Can it handle slash line comments? +Testing
    //
    // This is the description.
    resource "aws_instance" "example" {
      ami = "ami-12345"
    }
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                6 as usize,
                8 as usize,
                r#"2020-08-06 Can it handle slash line comments? +Testing

This is the description."#
                    .to_string()
            )
        );

        // Todo as block comment (end token on new line)
        let text = r#"
    variable "example" {
      default = "value"
    }

    /* TODO 2020-08-06 Can it handle block comments? +Testing

       This is the description.
     */
    resource "aws_instance" "example" {
      ami = "ami-12345"
    }
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                6 as usize,
                9 as usize,
                r#"2020-08-06 Can it handle block comments? +Testing

This is the description."#
                    .to_string()
            )
        );

        // Todo as block comment (end token on last line of todo)
        let text = r#"
    variable "example" {
      default = "value"
    }

    /* TODO 2020-08-06 Can it handle block comments? +Testing

       This is the description. */
    resource "aws_instance" "example" {
      ami = "ami-12345"
    }
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                6 as usize,
                8 as usize,
                r#"2020-08-06 Can it handle block comments? +Testing

This is the description."#
                    .to_string()
            )
        );

        // Todo with indented lines
        let text = r#"
    variable "example" {
      default = "value"
    }

    /* TODO 2020-08-06 Can it handle indented todos? +Testing

       This is a test todo with some indented lines:
         - This is an even more indented line.
     */

    resource "aws_instance" "example" {
      ami = "ami-12345"
    }
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                6 as usize,
                10 as usize,
                r#"2020-08-06 Can it handle indented todos? +Testing

This is a test todo with some indented lines:
  - This is an even more indented line."#
                    .to_string()
            )
        );
    }

    #[test]
    fn todo_inside_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &TERRAFORM);
        let text = r##"
variable "example" {
  default = "# TODO this is inside a string"
}

# TODO this is a real todo
resource "aws_instance" "example" {}
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_string() {
        let parser = crate::lang::Parser::new("TODO", &TERRAFORM);
        let text = r##"
variable "msg" {
  default = "hello \"
# TODO false positive
world"
}

# TODO real todo
"##;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }
}
