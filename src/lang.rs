pub mod dockerfile;
pub mod go;
pub mod makefile;
pub mod python;
pub mod rust;
pub mod tdz;
pub mod terraform;
pub mod typescript;
pub mod yaml;

pub const TODO_TOKEN: &str = "TODO";

// TODO #21 (A) 2026-03-12 Improve Parser edge case handling +fix
//
// 1. Regular strings - TODOs inside regular string literals (not raw strings)
//    are detected as real TODOs
// 2. Nested block comments - Rust allows /* /* */ */, parser stops at first */
// 3. TODO not at start of comment - "// Note: TODO fix" won't be detected
// 4. Python triple-single-quotes - only """ handled, not '''
// 5. Unicode boundary panic - line[prefix..] uses byte offsets, could panic
//    if prefix lands inside a multi-byte UTF-8 character
// 6. Inline comments - "let x = 1; // TODO change this" won't be detected
//    because line doesn't start with comment token
pub enum SyntaxRule<'a> {
    LineComment(&'a str),
    BlockComment(&'a str, &'a str),
    RawString(&'a str, &'a str),
    // String(&'a [u8]),
}

struct CommentFormat<T> {
    token: T,
    todo_token: String,
}

pub struct Parser {
    line_comment_delimiters: Vec<CommentFormat<String>>,
    block_comment_delimiters: Vec<CommentFormat<(String, String)>>,
    raw_string_delimiters: Vec<(String, String)>,
}

impl Parser {
    pub fn new(syntax_rules: &[SyntaxRule]) -> Self {
        let mut line_comment_delimiters = Vec::new();
        let mut block_comment_delimiters = Vec::new();
        let mut raw_string_delimiters = Vec::new();

        for rule in syntax_rules {
            use SyntaxRule::*;
            match rule {
                LineComment(token) => {
                    line_comment_delimiters.push(CommentFormat {
                        token: token.to_string(),
                        todo_token: format!("{} {}", token, TODO_TOKEN),
                    });
                }
                BlockComment(start, end) => {
                    block_comment_delimiters.push(CommentFormat {
                        token: (start.to_string(), end.to_string()),
                        todo_token: format!("{} {}", start, TODO_TOKEN),
                    });
                }
                RawString(start, end) => {
                    raw_string_delimiters.push((start.to_string(), end.to_string()));
                }
            }
        }

        Self {
            line_comment_delimiters,
            block_comment_delimiters,
            raw_string_delimiters,
        }
    }

    pub fn parse_todos(&self, text: &str) -> Vec<(usize, usize, String)> {
        let mut todos = Vec::<(usize, usize, String)>::new();

        let mut lines = text.lines().enumerate().peekable();
        'outer: while let Some((i, line)) = lines.next() {
            let trimmed = line.trim_start();

            for line_comment_delimiter in &self.line_comment_delimiters {
                if trimmed.starts_with(&line_comment_delimiter.todo_token) {
                    let mut todo: Vec<String> = Vec::new();
                    let mut end_line = i;

                    let v: Vec<&str> = line.split(TODO_TOKEN).collect();
                    todo.push(v[1].trim().to_string());
                    let prefix = v[0].len();

                    while let Some((j, peeked)) = lines.peek() {
                        let peeked_trimmed = peeked.trim_start();
                        // Stop if not a comment or if it's a new TODO
                        if !peeked_trimmed.starts_with(&line_comment_delimiter.token)
                            || peeked_trimmed.starts_with(&line_comment_delimiter.todo_token)
                        {
                            break;
                        }
                        end_line = *j;
                        let (_, line) = lines.next().unwrap();

                        if line.len() < prefix {
                            todo.push(String::new());
                        } else {
                            todo.push(line[prefix..].trim_end().to_owned());
                        }
                    }

                    todos.push((i + 1, end_line + 1, todo.join("\n")));
                    continue 'outer;
                }
            }

            for block_comment_delimiter in &self.block_comment_delimiters {
                if trimmed.starts_with(&block_comment_delimiter.todo_token) {
                    let mut todo: Vec<String> = Vec::new();

                    let v: Vec<&str> = line.split(TODO_TOKEN).collect();
                    let after_todo = v[1];
                    let prefix = v[0].len();

                    // Check if closing delimiter is on same line (single-line block comment)
                    if after_todo.contains(&block_comment_delimiter.token.1) {
                        let content = after_todo
                            .split(&block_comment_delimiter.token.1)
                            .next()
                            .unwrap();
                        todo.push(content.trim().to_string());
                        todos.push((i + 1, i + 1, todo.join("\n")));
                        continue 'outer;
                    }

                    todo.push(after_todo.trim().to_string());

                    while let Some((j, line)) = lines.next() {
                        if line.contains(&block_comment_delimiter.token.1) {
                            let v = line
                                .split(&block_comment_delimiter.token.1)
                                .collect::<Vec<&str>>();
                            if v[0].trim_end().len() > prefix {
                                todo.push(v[0][prefix..].trim_end().to_owned());
                            }

                            todos.push((i + 1, j + 1, todo.join("\n")));
                            continue 'outer;
                        }

                        if line.len() < prefix {
                            todo.push(String::new());
                        } else {
                            todo.push(line[prefix..].trim_end().to_owned());
                        }
                    }
                }
            }

            for raw_string_delimiter in &self.raw_string_delimiters {
                if trimmed.contains(&raw_string_delimiter.0) {
                    // Single-line raw string
                    if trimmed.contains(&raw_string_delimiter.1)
                        && trimmed.find(&raw_string_delimiter.0)
                            < trimmed.rfind(&raw_string_delimiter.1)
                    {
                        continue;
                    }
                    // Multi-line raw string
                    while let Some((_, line)) = lines.next() {
                        if line.contains(&raw_string_delimiter.1) {
                            continue 'outer;
                        }
                    }
                }
            }
        }

        todos
    }
}

// TODO #25 (A) 2026-03-12 Add Parser tests for edge cases +test
//
// - Unicode characters before TODO token
// - Mixed comment styles in same file
// - TODO token appearing multiple times on same line
// - Deeply indented TODOs
// - Empty lines within multi-line TODO comments

#[cfg(test)]
mod tests {
    use super::*;

    // Simple line comment syntax for testing
    const TEST_LINE_COMMENT: [SyntaxRule; 1] = [SyntaxRule::LineComment("//")];

    #[test]
    fn line_comment_basic_todo() {
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = "// TODO basic test\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "basic test".to_string()));
    }

    #[test]
    fn line_comment_multiline_todo() {
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = r#"// TODO multi-line test
// This is the second line
// This is the third line
let x = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                1,
                3,
                "multi-line test\nThis is the second line\nThis is the third line".to_string()
            )
        );
    }

    #[test]
    fn line_comment_todo_at_end_of_file() {
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = "let x = 1;\n// TODO at end of file";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (2, 2, "at end of file".to_string()));
    }

    #[test]
    fn line_comment_todo_at_end_of_file_multiline() {
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = r#"let x = 1;
// TODO at end of file
// with continuation"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (2, 3, "at end of file\nwith continuation".to_string())
        );
    }

    #[test]
    fn line_comment_empty_description() {
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = r#"// TODO
let x = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "".to_string()));
    }

    #[test]
    fn line_comment_adjacent_todos() {
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = r#"// TODO first todo
// TODO second todo
let x = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0], (1, 1, "first todo".to_string()));
        assert_eq!(todos[1], (2, 2, "second todo".to_string()));
    }

    #[test]
    fn line_comment_adjacent_todos_with_gap() {
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = r#"// TODO first todo
let x = 1;
// TODO second todo
let y = 2;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0], (1, 1, "first todo".to_string()));
        assert_eq!(todos[1], (3, 3, "second todo".to_string()));
    }

    // Block comment tests
    const TEST_BLOCK_COMMENT: [SyntaxRule; 1] = [SyntaxRule::BlockComment("/*", "*/")];

    #[test]
    fn block_comment_single_line() {
        let parser = Parser::new(&TEST_BLOCK_COMMENT);
        let text = "/* TODO single line */\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "single line".to_string()));
    }

    #[test]
    fn block_comment_multiline_closing_own_line() {
        let parser = Parser::new(&TEST_BLOCK_COMMENT);
        let text = r#"/* TODO multi-line
 * second line
 * third line
 */
let x = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (1, 4, "multi-line\nsecond line\nthird line".to_string())
        );
    }

    #[test]
    fn block_comment_multiline_closing_after_content() {
        let parser = Parser::new(&TEST_BLOCK_COMMENT);
        let text = r#"/* TODO multi-line
 * second line
 * third line */
let x = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (1, 3, "multi-line\nsecond line\nthird line".to_string())
        );
    }

    #[test]
    fn block_comment_todo_at_end_of_file() {
        let parser = Parser::new(&TEST_BLOCK_COMMENT);
        let text = r#"let x = 1;
/* TODO at end of file
 * more content
 */"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (2, 4, "at end of file\nmore content".to_string())
        );
    }

    // Raw string tests
    const TEST_WITH_RAW_STRING: [SyntaxRule; 2] = [
        SyntaxRule::LineComment("//"),
        SyntaxRule::RawString("`", "`"),
    ];

    #[test]
    fn raw_string_todo_inside_ignored() {
        let parser = Parser::new(&TEST_WITH_RAW_STRING);
        let text = r#"let x = `
// TODO this should be ignored
`;
// TODO this should be found
let y = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (4, 4, "this should be found".to_string()));
    }

    #[test]
    fn raw_string_single_line() {
        let parser = Parser::new(&TEST_WITH_RAW_STRING);
        let text = r#"let x = `// TODO fake`;
// TODO real
let y = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (2, 2, "real".to_string()));
    }

    #[test]
    fn raw_string_todo_after_detected() {
        let parser = Parser::new(&TEST_WITH_RAW_STRING);
        let text = r#"let x = `raw string content`;
// TODO after raw string
let y = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (2, 2, "after raw string".to_string()));
    }

    #[test]
    fn raw_string_delimiter_in_comment() {
        let parser = Parser::new(&TEST_WITH_RAW_STRING);
        let text = r#"// TODO mentions ` backtick
let x = 1;
// TODO second todo
let y = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0], (1, 1, "mentions ` backtick".to_string()));
        assert_eq!(todos[1], (3, 3, "second todo".to_string()));
    }
}
