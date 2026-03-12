use super::SyntaxRule;

pub const TERRAFORM: [SyntaxRule; 3] = [
    SyntaxRule::LineComment("#"),
    SyntaxRule::LineComment("//"),
    SyntaxRule::BlockComment("/*", "*/"),
];

pub fn extract_todos(text: &str) -> Vec<crate::RawTodo> {
    let parser = super::Parser::new(&TERRAFORM);
    parser.parse_todos(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_todos() {
        let parser = crate::lang::Parser::new(&TERRAFORM);

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
            parser.parse_todos(text)[0],
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
            parser.parse_todos(text)[0],
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
     *
     * This is the description.
     */
    resource "aws_instance" "example" {
      ami = "ami-12345"
    }
"#;
        assert_eq!(
            parser.parse_todos(text)[0],
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
     *
     * This is the description. */
    resource "aws_instance" "example" {
      ami = "ami-12345"
    }
"#;
        assert_eq!(
            parser.parse_todos(text)[0],
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
     *
     * This is a test todo with some indented lines:
     *   - This is an even more indented line.
     */

    resource "aws_instance" "example" {
      ami = "ami-12345"
    }
"#;
        assert_eq!(
            parser.parse_todos(text)[0],
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
}
