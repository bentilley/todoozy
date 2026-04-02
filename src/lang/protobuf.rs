use super::SyntaxRule;

pub const PROTOBUF: [SyntaxRule; 3] = [
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
        let parser = crate::lang::Parser::new("TODO", &PROTOBUF);

        // Todo as line comment
        let text = r#"
syntax = "proto3";

// TODO 2024-09-02 Add user authentication fields +feature
//
// Need to add auth token and expiry fields.
message User {
    string name = 1;
}
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                4 as usize,
                6 as usize,
                r#"2024-09-02 Add user authentication fields +feature

Need to add auth token and expiry fields."#
                    .to_string()
            )
        );

        // Todo as block comment
        let text = r#"
syntax = "proto3";

/* TODO 2024-09-02 Deprecate old API +cleanup

This service is being replaced by v2.
*/
service OldService {
    rpc GetData (Request) returns (Response);
}
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                4 as usize,
                7 as usize,
                r#"2024-09-02 Deprecate old API +cleanup

This service is being replaced by v2."#
                    .to_string()
            )
        );

        // Single-line block comment
        let text = r#"
syntax = "proto3";
/* TODO 2024-09-02 Add validation +improvement */
message Request {}
"#;
        assert_eq!(
            parser.parse_str(text)[0],
            (
                3 as usize,
                3 as usize,
                "2024-09-02 Add validation +improvement".to_string()
            )
        );
    }

    #[test]
    fn todo_inside_string_ignored() {
        let parser = crate::lang::Parser::new("TODO", &PROTOBUF);
        let text = r#"
syntax = "proto3";
string msg = "// TODO this is inside a string";

// TODO this is a real todo
message Request {}
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "this is a real todo".to_string());
    }

    #[test]
    fn escaped_quote_in_string() {
        let parser = crate::lang::Parser::new("TODO", &PROTOBUF);
        let text = r#"
string msg = "hello \"
// TODO false positive
world";

// TODO real todo
"#;
        let todos = parser.parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].2, "real todo".to_string());
    }
}
