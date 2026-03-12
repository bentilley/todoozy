pub mod go;
pub mod python;
pub mod rust;
pub mod tdz;
pub mod typescript;

pub const TODO_TOKEN: &str = "TODO";

// TODO #21 (A) 2026-03-12 Improve Parser edge case handling +fix
//
// 1. Regular strings - TODOs inside regular string literals (not raw strings)
//    are detected as real TODOs
// 2. Raw string delimiter in comments - a comment mentioning r#" triggers
//    raw-string-skip mode incorrectly
// 3. Single-line block comments - /* TODO foo */ on one line doesn't parse
//    correctly, expects closing delimiter on subsequent line
// 4. Nested block comments - Rust allows /* /* */ */, parser stops at first */
// 5. TODO not at start of comment - "// Note: TODO fix" won't be detected
// 6. Python triple-single-quotes - only """ handled, not '''
// 7. Unicode boundary panic - line[prefix..] uses byte offsets, could panic
//    if prefix lands inside a multi-byte UTF-8 character
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

            for raw_string_delimiter in &self.raw_string_delimiters {
                if trimmed.contains(&raw_string_delimiter.0) {
                    while let Some((_, line)) = lines.next() {
                        if line.contains(&raw_string_delimiter.1) {
                            continue 'outer;
                        }
                    }
                }
            }

            for line_comment_delimiter in &self.line_comment_delimiters {
                if trimmed.starts_with(&line_comment_delimiter.todo_token) {
                    let mut todo: Vec<String> = Vec::new();

                    let v: Vec<&str> = line.split(TODO_TOKEN).collect();
                    todo.push(v[1].trim().to_string());
                    let prefix = v[0].len();

                    while let Some((j, line)) = lines.peek() {
                        if !line.trim_start().starts_with(&line_comment_delimiter.token) {
                            todos.push((i + 1, *j, todo.join("\n")));
                            continue 'outer;
                        }
                        let (_, line) = lines.next().unwrap();

                        if line.len() < prefix {
                            todo.push(String::new());
                        } else {
                            todo.push(line[prefix..].trim_end().to_owned());
                        }
                    }
                }
            }

            for block_comment_delimiter in &self.block_comment_delimiters {
                if trimmed.starts_with(&block_comment_delimiter.todo_token) {
                    let mut todo: Vec<String> = Vec::new();

                    let v: Vec<&str> = line.split(TODO_TOKEN).collect();
                    todo.push(v[1].trim().to_string());
                    let prefix = v[0].len();

                    while let Some((j, line)) = lines.next() {
                        if line.contains(&block_comment_delimiter.token.1) {
                            let mut end_line = j;
                            let v = line
                                .split(&block_comment_delimiter.token.1)
                                .collect::<Vec<&str>>();
                            if v[0].trim_end().len() > prefix {
                                end_line += 1;
                                todo.push(v[0][prefix..].trim_end().to_owned());
                            }

                            todos.push((i + 1, end_line, todo.join("\n")));
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
        }

        todos
    }
}

// TODO #22 (A) 2026-03-12 Add Parser tests for line comments +test
//
// - Basic TODO detection
// - Multi-line TODO with continuation comments
// - TODO at end of file (no trailing newline) (this is currently broken for sure)
// - TODO with empty description
// - Adjacent TODOs (two TODOs back-to-back)

// TODO #23 (A) 2026-03-12 Add Parser tests for block comments +test
//
// - Single-line block comment: /* TODO foo */
// - Multi-line block comment with closing on own line
// - Multi-line block comment with closing after content
// - Block comment TODO at end of file

// TODO #24 (A) 2026-03-12 Add Parser tests for raw strings +test
//
// - TODO inside raw string is ignored
// - Raw string on single line (open and close same line)
// - TODO after raw string is detected
// - Raw string delimiter mentioned in comment (false positive case)

// TODO #25 (A) 2026-03-12 Add Parser tests for edge cases +test
//
// - Unicode characters before TODO token
// - Mixed comment styles in same file
// - TODO token appearing multiple times on same line
// - Deeply indented TODOs
// - Empty lines within multi-line TODO comments

#[test]
fn test_parser() {}
