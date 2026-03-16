use regex::Regex;

pub mod dockerfile;
pub mod go;
pub mod makefile;
pub mod markdown;
pub mod protobuf;
pub mod python;
pub mod rust;
pub mod sh;
pub mod tdz;
pub mod terraform;
pub mod typescript;
pub mod yaml;

pub const TODO_TOKEN: &str = "TODO";

// TODO #33 (B) 2026-03-12 Handle TODOs inside regular string literals +fix
//
// TODOs inside regular string literals (not raw strings) are detected as real TODOs.
// Need to add string literal parsing to skip content inside quotes.

// TODO #35 (B) 2026-03-12 Handle nested block comments +fix
//
// Rust allows nested block comments like /* /* */ */, but parser stops at first */.
// Need to track nesting depth when parsing block comments.

// TODO #37 (C) 2026-03-12 Detect inline comments +fix
//
// "let x = 1; // TODO change this" won't be detected because line doesn't start
// with comment token. Would need to scan for comment tokens mid-line.

pub enum SyntaxRule<'a> {
    LineComment(&'a str),
    BlockComment(&'a str, &'a str),
    MultiLineString(&'a str, &'a str),
    // String(&'a [u8]),
}

enum ParseResult {
    Todo((usize, usize, String)),
    Success,
    NoMatch,
}

trait ParseRule {
    fn try_parse(
        &self,
        lines: &mut std::iter::Peekable<std::iter::Enumerate<std::str::Lines>>,
    ) -> ParseResult;
}

struct LineCommentRule {
    token: String,
    todo_regex: Regex,
}

impl LineCommentRule {
    fn new(token: &str) -> Self {
        let pattern = format!(r"^\s*{}\s*{}\b", regex::escape(token), TODO_TOKEN);
        Self {
            token: token.to_string(),
            todo_regex: Regex::new(&pattern).unwrap(),
        }
    }
}

impl ParseRule for LineCommentRule {
    fn try_parse(
        &self,
        lines: &mut std::iter::Peekable<std::iter::Enumerate<std::str::Lines>>,
    ) -> ParseResult {
        match lines.peek() {
            Some((_, peeked)) if self.todo_regex.is_match(peeked.trim_start()) => {
                let (i, line) = lines.next().unwrap();
                let mut todo: Vec<String> = Vec::new();
                let mut end_line = i;

                let v: Vec<&str> = line.splitn(2, TODO_TOKEN).collect();
                todo.push(v[1].trim().to_string());
                let prefix = v[0].len();

                while let Some((j, peeked)) = lines.peek() {
                    let peeked_trimmed = peeked.trim_start();
                    // Stop if not a comment or if it's a new TODO
                    if !peeked_trimmed.starts_with(&self.token)
                        || self.todo_regex.is_match(peeked_trimmed)
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

                return ParseResult::Todo((i + 1, end_line + 1, todo.join("\n")));
            }
            _ => return ParseResult::NoMatch,
        }
    }
}

struct BlockCommentRule {
    // start_token: String,
    end_token: String,
    todo_regex: Regex,
}

impl BlockCommentRule {
    fn new(start: &str, end: &str) -> Self {
        let pattern = format!(r"^\s*{}\s*{}\b", regex::escape(start), TODO_TOKEN);
        Self {
            // start_token: start.to_string(),
            end_token: end.to_string(),
            todo_regex: Regex::new(&pattern).unwrap(),
        }
    }
}

impl ParseRule for BlockCommentRule {
    fn try_parse(
        &self,
        lines: &mut std::iter::Peekable<std::iter::Enumerate<std::str::Lines>>,
    ) -> ParseResult {
        match lines.peek() {
            Some((_, peeked)) if self.todo_regex.is_match(peeked.trim_start()) => {
                let (i, line) = lines.next().unwrap();
                let mut todo: Vec<String> = Vec::new();

                let v: Vec<&str> = line.splitn(2, TODO_TOKEN).collect();
                let after_todo = v[1];

                // Check if closing delimiter is on same line (single-line block comment)
                if after_todo.contains(&self.end_token) {
                    let content = after_todo.split(&self.end_token).next().unwrap();
                    todo.push(content.trim().to_string());
                    return ParseResult::Todo((i + 1, i + 1, todo.join("\n")));
                }

                todo.push(after_todo.trim().to_string());

                // Prefix is None until we see the first non-empty description line
                let mut prefix: Option<usize> = None;

                while let Some((j, line)) = lines.next() {
                    if line.contains(&self.end_token) {
                        let v = line.split(&self.end_token).collect::<Vec<&str>>();
                        let content = v[0];
                        if !content.trim().is_empty() {
                            let content_start = content.len() - content.trim_start().len();
                            let p = prefix.unwrap_or(content_start);
                            if content.len() > p {
                                todo.push(content[p..].trim_end().to_owned());
                            }
                        }

                        return ParseResult::Todo((i + 1, j + 1, todo.join("\n")));
                    }

                    if line.trim().is_empty() {
                        todo.push(String::new());
                    } else {
                        // Set prefix on first line with actual content
                        let content_start = line.len() - line.trim_start().len();
                        let p = *prefix.get_or_insert(content_start);
                        if line.len() > p {
                            todo.push(line[p..].trim_end().to_owned());
                        } else {
                            todo.push(String::new());
                        }
                    }
                }

                return ParseResult::Todo((i + 1, i + todo.len(), todo.join("\n")));
            }
            _ => return ParseResult::NoMatch,
        }
    }
}

struct MultiLineStringRule {
    start_token: String,
    end_token: String,
}

impl MultiLineStringRule {
    fn new(start: &str, end: &str) -> Self {
        Self {
            start_token: start.to_string(),
            end_token: end.to_string(),
        }
    }
}

impl ParseRule for MultiLineStringRule {
    fn try_parse(
        &self,
        lines: &mut std::iter::Peekable<std::iter::Enumerate<std::str::Lines>>,
    ) -> ParseResult {
        match lines.peek() {
            Some((_, peeked)) if peeked.contains(&self.start_token) => {
                let (_, line) = lines.next().unwrap();
                let trimmed = line.trim_start();

                // Single-line raw string
                if trimmed.contains(&self.end_token)
                    && trimmed.find(&self.start_token) < trimmed.rfind(&self.end_token)
                {
                    return ParseResult::Success;
                }

                // Multi-line raw string
                while let Some((_, line)) = lines.next() {
                    if line.contains(&self.end_token) {
                        return ParseResult::Success;
                    }
                }

                return ParseResult::Success;
            }
            _ => return ParseResult::NoMatch,
        }
    }
}

pub struct Parser {
    parse_rules: Vec<Box<dyn ParseRule>>,
}

impl Parser {
    pub fn new(syntax_rules: &[SyntaxRule]) -> Self {
        let mut parse_rules = Vec::<Box<dyn ParseRule>>::new();

        for rule in syntax_rules {
            use SyntaxRule::*;
            match rule {
                LineComment(token) => {
                    parse_rules.push(Box::new(LineCommentRule::new(token)));
                }
                BlockComment(start, end) => {
                    parse_rules.push(Box::new(BlockCommentRule::new(start, end)));
                }
                MultiLineString(start, end) => {
                    parse_rules.push(Box::new(MultiLineStringRule::new(start, end)));
                }
            }
        }

        Self { parse_rules }
    }

    pub fn parse_todos(&self, text: &str) -> Vec<(usize, usize, String)> {
        let mut todos = Vec::<(usize, usize, String)>::new();

        let mut lines = text.lines().enumerate().peekable();
        'outer: while let Some(_) = lines.peek() {
            for rule in &self.parse_rules {
                use ParseResult::*;
                match rule.try_parse(&mut lines) {
                    Todo(todo) => {
                        todos.push(todo);
                        continue 'outer;
                    }
                    Success => continue 'outer,
                    NoMatch => continue,
                }
            }
            lines.next(); // No rule matched this line, skip it
        }

        todos
    }
}

// TODO #42 (C) 2026-03-12 Test deeply indented TODOs +test
//
// Ensure deeply indented TODO comments are parsed correctly.

#[cfg(test)]
mod tests {
    use super::*;

    // Line comment tests
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

    #[test]
    fn line_comment_todo_in_title() {
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = r#"// TODO (B) Handle TODOs inside TODO title
let x = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (1, 1, "(B) Handle TODOs inside TODO title".to_string())
        );
    }

    #[test]
    fn line_comments_description_starts_with_todos() {
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = r#"// TODO (B) Handle TODOs inside TODO title
//
// TODOs should not start new TODO unless "TODO" followed by whitespace
let x = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (1, 3, "(B) Handle TODOs inside TODO title\n\nTODOs should not start new TODO unless \"TODO\" followed by whitespace".to_string())
        );
    }

    #[test]
    fn line_comment_multiple_spaces_before_todo() {
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = "//    TODO with extra spaces\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "with extra spaces".to_string()));
    }

    #[test]
    fn line_comment_tab_before_todo() {
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = "//\tTODO with tab\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "with tab".to_string()));
    }

    #[test]
    fn line_comment_no_space_before_todo() {
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = "//TODO no space\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "no space".to_string()));
    }

    #[test]
    fn line_comment_todolist_not_detected() {
        // Word boundary prevents matching "TODOLIST"
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = "// TODOLIST is not a todo\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 0);
    }

    #[test]
    fn line_comment_empty_line_preserved() {
        let parser = Parser::new(&TEST_LINE_COMMENT);
        let text = r#"// TODO title line
//
// continuation after empty line
let x = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (1, 3, "title line\n\ncontinuation after empty line".to_string())
        );
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
   second line
   third line
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
   second line
   third line */
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
   more content
 */"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (2, 4, "at end of file\nmore content".to_string()));
    }

    #[test]
    fn block_comment_asterisk_border_not_stripped() {
        // Documents that * borders in block comments are NOT stripped
        let parser = Parser::new(&TEST_BLOCK_COMMENT);
        let text = r#"/* TODO with asterisk border
 * this line has asterisk
 * so does this
 */"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                1,
                4,
                "with asterisk border\n* this line has asterisk\n* so does this".to_string()
            )
        );
    }

    #[test]
    fn block_comment_todo_in_description() {
        let parser = Parser::new(&TEST_BLOCK_COMMENT);
        let text = r#"/* TODO (B) Handle TODOs inside TODO description
   This TODO should not start a new todo.
*/"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                1,
                3,
                "(B) Handle TODOs inside TODO description\nThis TODO should not start a new todo."
                    .to_string()
            )
        );
    }

    #[test]
    fn block_comment_left_edge_content() {
        // Content starts at left edge, not aligned to comment opener
        let parser = Parser::new(&TEST_BLOCK_COMMENT);
        let text = r#"/* TODO left edge test

Content starts at the left edge.
  - indented item
*/"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                1,
                5,
                "left edge test\n\nContent starts at the left edge.\n  - indented item".to_string()
            )
        );
    }

    // Multi-line string tests
    const TEST_WITH_MULTI_LINE_STRING: [SyntaxRule; 2] = [
        SyntaxRule::LineComment("//"),
        SyntaxRule::MultiLineString("`", "`"),
    ];

    #[test]
    fn multi_line_string_todo_inside_ignored() {
        let parser = Parser::new(&TEST_WITH_MULTI_LINE_STRING);
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
    fn multi_line_string_single_line() {
        let parser = Parser::new(&TEST_WITH_MULTI_LINE_STRING);
        let text = r#"let x = `// TODO fake`;
// TODO real
let y = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (2, 2, "real".to_string()));
    }

    #[test]
    fn multi_line_string_todo_after_detected() {
        let parser = Parser::new(&TEST_WITH_MULTI_LINE_STRING);
        let text = r#"let x = `raw string content`;
// TODO after raw string
let y = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (2, 2, "after raw string".to_string()));
    }

    #[test]
    fn multi_line_string_delimiter_in_comment() {
        let parser = Parser::new(&TEST_WITH_MULTI_LINE_STRING);
        let text = r#"// TODO mentions ` backtick
let x = 1;
// TODO second todo
let y = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0], (1, 1, "mentions ` backtick".to_string()));
        assert_eq!(todos[1], (3, 3, "second todo".to_string()));
    }

    // Mixed comment style tests
    const TEST_MIXED_COMMENTS: [SyntaxRule; 2] = [
        SyntaxRule::LineComment("//"),
        SyntaxRule::BlockComment("/*", "*/"),
    ];

    #[test]
    fn mixed_comment_styles_both_detected() {
        let parser = Parser::new(&TEST_MIXED_COMMENTS);
        let text = r#"// TODO first in line comment
let x = 1;
/* TODO second in block comment */
let y = 2;
// TODO third back to line comment
let z = 3;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 3);
        assert_eq!(todos[0], (1, 1, "first in line comment".to_string()));
        assert_eq!(todos[1], (3, 3, "second in block comment".to_string()));
        assert_eq!(todos[2], (5, 5, "third back to line comment".to_string()));
    }

    #[test]
    fn mixed_comment_styles_multiline() {
        let parser = Parser::new(&TEST_MIXED_COMMENTS);
        let text = r#"// TODO line comment todo
// with continuation
let x = 1;
/* TODO block comment todo

with continuation
*/
let y = 2;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(
            todos[0],
            (1, 2, "line comment todo\nwith continuation".to_string())
        );
        assert_eq!(
            todos[1],
            (4, 7, "block comment todo\n\nwith continuation".to_string())
        );
    }
}
