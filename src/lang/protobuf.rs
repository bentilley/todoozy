use super::SyntaxRule;

pub const PROTOBUF: [SyntaxRule; 2] = [
    SyntaxRule::LineComment("//"),
    SyntaxRule::BlockComment("/*", "*/"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_todos() {
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
            parser.parse_todos(text)[0],
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
            parser.parse_todos(text)[0],
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
            parser.parse_todos(text)[0],
            (
                3 as usize,
                3 as usize,
                "2024-09-02 Add validation +improvement".to_string()
            )
        );
    }
}
