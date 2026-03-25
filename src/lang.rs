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

// TODO #70 (B) 2026-03-25 Fix same project twice bug +fix
//
// Includeing a project tag twice e.g. +tag +tag actually tags the todo twice, this should be
// filtered for unique tags only.

/* TODO #58 (D) 2026-03-22 Inline comments: single-line only with code capture +parser +refs

Change inline comment (end-of-line) parsing behavior:

1. Inline comments never aggregate with following lines - they are always
   single-line TODOs. This avoids confusing behavior where subsequent
   comment lines might unexpectedly merge.

2. Automatically capture the code preceding the inline comment into the
   The TODO's description as a code block. Example:

     x += 1  // TODO fix increment bug +fix

   Becomes:
     TODO Fix increment bug +fix

     `x += 1`

This makes inline TODOs natural "reference" candidates - quick breadcrumbs
with automatic context. Users can use `#id` or `&id` for primary/reference.
*/

pub enum SyntaxRule {
    LineComment(&'static [u8]),
    BlockComment(&'static [u8], &'static [u8]),
    SkipDelimited(&'static [u8], &'static [u8]),
    SkipDelimitedWithEscape(&'static [u8], &'static [u8], u8),
    Custom(for<'a> fn(u8, &'a [u8], usize) -> Option<(usize, usize, Option<Comment<'a>>)>),
}

pub enum Comment<'a> {
    Line(&'a str, usize),
    Block(&'a str, usize, usize),
}

struct CommentParser<'a> {
    syntax_rules: &'static [SyntaxRule],
    text: &'a [u8],
    len: usize,
    position: usize,
    line_number: usize,
}

impl<'a> CommentParser<'a> {
    fn new(syntax_rules: &'static [SyntaxRule], text: &'a str) -> Self {
        Self {
            syntax_rules,
            text: text.as_bytes(),
            len: text.as_bytes().len(),
            position: 0,
            line_number: 1,
        }
    }
}

impl<'a> Iterator for CommentParser<'a> {
    type Item = Comment<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.len {
            return None;
        }

        // TODO #50 (E) 2026-03-19 Optimise comment parsing with trie? +perf
        //
        // Current implementation checks all rules at every byte. Could try building a trie of
        // comment start tokens, to exit early from the rule checking. E.g. If '//' doesn't match
        // a location, you can skip checking '/*' and other rules that start with '/'.
        'outer: while self.position < self.len {
            let current_byte = self.text[self.position];
            for rule in self.syntax_rules {
                match rule {
                    SyntaxRule::LineComment(token) => {
                        if current_byte == token[0] && self.text[self.position..].starts_with(token)
                        {
                            let comment_line = self.line_number;
                            self.position += token.len();
                            let start = self.position;
                            while self.position < self.len && self.text[self.position] != b'\n' {
                                self.position += 1;
                            }
                            let line =
                                std::str::from_utf8(&self.text[start..self.position]).unwrap_or("");

                            if self.position < self.len {
                                self.position += 1; // skip newline
                                self.line_number += 1;
                            }

                            return Some(Comment::Line(line, comment_line));
                        }
                    }

                    SyntaxRule::BlockComment(start_token, end_token) => {
                        if current_byte == start_token[0]
                            && self.text[self.position..].starts_with(start_token)
                        {
                            self.position += start_token.len();
                            let content_start = self.position;
                            let start_line = self.line_number;

                            let mut depth = 1;
                            while self.position < self.len && depth > 0 {
                                if self.text[self.position..].starts_with(start_token) {
                                    depth += 1;
                                    self.position += start_token.len();
                                } else if self.text[self.position..].starts_with(end_token) {
                                    depth -= 1;
                                    self.position += end_token.len();
                                } else {
                                    if self.text[self.position] == b'\n' {
                                        self.line_number += 1;
                                    }
                                    self.position += 1;
                                }
                            }

                            let end_line = self.line_number;
                            let content_end = self.position - end_token.len();
                            let content =
                                std::str::from_utf8(&self.text[content_start..content_end])
                                    .unwrap_or("");
                            return Some(Comment::Block(content, start_line, end_line));
                        }
                    }

                    SyntaxRule::SkipDelimited(start_delim, end_delim) => {
                        if current_byte == start_delim[0]
                            && self.text[self.position..].starts_with(start_delim)
                        {
                            self.position += start_delim.len();

                            while self.position < self.len
                                && !self.text[self.position..].starts_with(end_delim)
                            {
                                if self.text[self.position] == b'\n' {
                                    self.line_number += 1;
                                }
                                self.position += 1;
                            }

                            if self.position < self.len {
                                self.position += end_delim.len();
                            }

                            continue 'outer;
                        }
                    }

                    SyntaxRule::SkipDelimitedWithEscape(start_delim, end_delim, escape_char) => {
                        if current_byte == start_delim[0]
                            && self.text[self.position..].starts_with(start_delim)
                        {
                            self.position += start_delim.len();

                            while self.position < self.len {
                                // Check for escape character - skip it and the next byte
                                if self.text[self.position] == *escape_char {
                                    self.position += 1;
                                    if self.position < self.len {
                                        if self.text[self.position] == b'\n' {
                                            self.line_number += 1;
                                        }
                                        self.position += 1;
                                    }
                                    continue;
                                }

                                if self.text[self.position..].starts_with(end_delim) {
                                    break;
                                }

                                if self.text[self.position] == b'\n' {
                                    self.line_number += 1;
                                }
                                self.position += 1;
                            }

                            if self.position < self.len {
                                self.position += end_delim.len();
                            }

                            continue 'outer;
                        }
                    }

                    SyntaxRule::Custom(parse_fn) => {
                        if let Some((bytes_consumed, lines_seen, comment)) =
                            parse_fn(current_byte, self.text, self.position)
                        {
                            self.position += bytes_consumed;
                            self.line_number += lines_seen;

                            if let Some(c) = comment {
                                return Some(c);
                            }

                            continue 'outer;
                        }
                    }
                }
            }

            // No rule matched - advance by one byte
            if self.text[self.position] == b'\n' {
                self.line_number += 1;
            }
            self.position += 1;
        }

        None
    }
}

pub struct Parser<'a> {
    todo_token: &'a str,
    syntax_rules: &'static [SyntaxRule],
}

impl<'a> Parser<'a> {
    pub fn new(todo_token: &'a str, syntax_rules: &'static [SyntaxRule]) -> Self {
        Self {
            todo_token,
            syntax_rules,
        }
    }

    fn is_todo_start(&self, text: &str) -> bool {
        let trimmed = text.trim_start();

        if !trimmed.starts_with(self.todo_token) {
            return false;
        }

        // Check for word boundary after token
        let after_token = &trimmed[self.todo_token.len()..];
        after_token.is_empty()
            || after_token.starts_with(char::is_whitespace)
            || !after_token.chars().next().unwrap().is_alphanumeric()
    }

    pub fn parse_todos(&self, text: &str) -> Vec<(usize, usize, String)> {
        let mut todos = Vec::new();

        let mut comments = CommentParser::new(self.syntax_rules, text).peekable();
        while let Some(comment) = comments.next() {
            use Comment::*;
            match comment {
                Line(text, line_number) => {
                    if !self.is_todo_start(text) {
                        continue;
                    }

                    let mut todo_text = text
                        .splitn(2, &self.todo_token)
                        .nth(1)
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let mut end_line_number = line_number;

                    let mut indent_prefix_len = None;
                    while let Some(next_comment) = comments.peek() {
                        let is_continuation_line = match next_comment {
                            Line(next_text, next_line_number) => {
                                !self.is_todo_start(next_text)
                                    && *next_line_number == end_line_number + 1
                            }
                            Block(_, _, _) => false,
                        };
                        if !is_continuation_line {
                            break;
                        }

                        if let Some(Line(next_text, next_line_number)) = comments.next() {
                            end_line_number = next_line_number;
                            todo_text.push_str("\n");

                            // Skip empty lines without updating indent prefix
                            if next_text.trim().is_empty() {
                                continue;
                            }

                            let content_start = next_text.len() - next_text.trim_start().len();
                            let prefix_len = *indent_prefix_len.get_or_insert(content_start);
                            if content_start >= prefix_len {
                                todo_text.push_str(&next_text[prefix_len..].trim_end());
                            } else {
                                todo_text.push_str(next_text.trim());
                            }
                        }
                    }

                    todos.push((line_number, end_line_number, todo_text));
                }
                Block(text, start_line, end_line) => {
                    if !self.is_todo_start(text) {
                        continue;
                    }

                    let mut lines = text
                        .splitn(2, &self.todo_token)
                        .nth(1)
                        .unwrap_or("")
                        .lines();

                    let mut todo_text = String::new();
                    if let Some(first_line) = lines.next() {
                        todo_text.push_str(first_line.trim());
                    }

                    let mut indent_prefix_len = None;
                    for line in lines {
                        if line.trim().is_empty() {
                            todo_text.push_str("\n");
                            continue;
                        }
                        let content_start = line.len() - line.trim_start().len();
                        let prefix_len = *indent_prefix_len.get_or_insert(content_start);
                        todo_text.push_str("\n");
                        if content_start >= prefix_len {
                            todo_text.push_str(&line[prefix_len..].trim_end());
                        } else {
                            todo_text.push_str(line.trim());
                        }
                    }

                    // Remove trailing newlines
                    while todo_text.ends_with('\n') {
                        todo_text.pop();
                    }

                    todos.push((start_line, end_line, todo_text));
                }
            }
        }

        todos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Line comment tests
    const TEST_LINE_COMMENT: [SyntaxRule; 1] = [SyntaxRule::LineComment(b"//")];

    #[test]
    fn line_comment_basic_todo() {
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
        let text = "// TODO basic test\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "basic test".to_string()));
    }

    #[test]
    fn line_comment_multiline_todo() {
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
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
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
        let text = "let x = 1;\n// TODO at end of file";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (2, 2, "at end of file".to_string()));
    }

    #[test]
    fn line_comment_todo_at_end_of_file_multiline() {
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
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
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
        let text = r#"// TODO
let x = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "".to_string()));
    }

    #[test]
    fn line_comment_adjacent_todos() {
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
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
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
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
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
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
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
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
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
        let text = "//    TODO with extra spaces\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "with extra spaces".to_string()));
    }

    #[test]
    fn line_comment_tab_before_todo() {
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
        let text = "//\tTODO with tab\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "with tab".to_string()));
    }

    #[test]
    fn line_comment_no_space_before_todo() {
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
        let text = "//TODO no space\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "no space".to_string()));
    }

    #[test]
    fn line_comment_todolist_not_detected() {
        // Word boundary prevents matching "TODOLIST"
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
        let text = "// TODOLIST is not a todo\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 0);
    }

    #[test]
    fn line_comment_empty_line_preserved() {
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
        let text = r#"// TODO title line
//
// continuation after empty line
let x = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                1,
                3,
                "title line\n\ncontinuation after empty line".to_string()
            )
        );
    }

    #[test]
    fn line_comment_deeply_indented() {
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
        let text = r#"fn foo() {
    if condition {
        while x {
            // TODO deeply indented task
            // with continuation line
            do_something();
        }
    }
}"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                4,
                5,
                "deeply indented task\nwith continuation line".to_string()
            )
        );
    }

    #[test]
    fn line_comment_inline_todo() {
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
        let text = "let x = 1; // TODO change this\nlet y = 2;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "change this".to_string()));
    }

    #[test]
    fn line_comment_inline_todo_with_continuation() {
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
        let text = r#"let x = 1; // TODO inline with continuation
// this continues the todo
let y = 2;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                1,
                2,
                "inline with continuation\nthis continues the todo".to_string()
            )
        );
    }

    #[test]
    fn line_comment_multiple_inline_todos() {
        let parser = Parser::new("TODO", &TEST_LINE_COMMENT);
        let text = r#"let x = 1; // TODO first inline
let y = 2; // TODO second inline
let z = 3;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0], (1, 1, "first inline".to_string()));
        assert_eq!(todos[1], (2, 2, "second inline".to_string()));
    }

    // Block comment tests
    const TEST_BLOCK_COMMENT: [SyntaxRule; 1] = [SyntaxRule::BlockComment(b"/*", b"*/")];

    #[test]
    fn block_comment_single_line() {
        let parser = Parser::new("TODO", &TEST_BLOCK_COMMENT);
        let text = "/* TODO single line */\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "single line".to_string()));
    }

    #[test]
    fn block_comment_multiline_closing_own_line() {
        let parser = Parser::new("TODO", &TEST_BLOCK_COMMENT);
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
        let parser = Parser::new("TODO", &TEST_BLOCK_COMMENT);
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
        let parser = Parser::new("TODO", &TEST_BLOCK_COMMENT);
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
        let parser = Parser::new("TODO", &TEST_BLOCK_COMMENT);
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
        let parser = Parser::new("TODO", &TEST_BLOCK_COMMENT);
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
        let parser = Parser::new("TODO", &TEST_BLOCK_COMMENT);
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

    #[test]
    fn block_comment_nested_simple() {
        let parser = Parser::new("TODO", &TEST_BLOCK_COMMENT);
        let text = "/* TODO outer /* inner */ still outer */\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (1, 1, "outer /* inner */ still outer".to_string())
        );
    }

    #[test]
    fn block_comment_nested_multiline() {
        let parser = Parser::new("TODO", &TEST_BLOCK_COMMENT);
        let text = r#"/* TODO nested multiline
   /* this is nested
      and spans lines
   */
   back to outer
*/
let x = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                1,
                6,
                "nested multiline\n/* this is nested\n   and spans lines\n*/\nback to outer"
                    .to_string()
            )
        );
    }

    #[test]
    fn block_comment_nested_two_levels() {
        let parser = Parser::new("TODO", &TEST_BLOCK_COMMENT);
        let text = "/* TODO /* level 1 /* level 2 */ back to 1 */ outer */\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                1,
                1,
                "/* level 1 /* level 2 */ back to 1 */ outer".to_string()
            )
        );
    }

    #[test]
    fn block_comment_nested_does_not_affect_next_comment() {
        let parser = Parser::new("TODO", &TEST_BLOCK_COMMENT);
        let text = r#"/* TODO first with /* nested */ content */
/* TODO second todo */
let x = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(
            todos[0],
            (1, 1, "first with /* nested */ content".to_string())
        );
        assert_eq!(todos[1], (2, 2, "second todo".to_string()));
    }

    #[test]
    fn block_comment_nested_empty_inner() {
        let parser = Parser::new("TODO", &TEST_BLOCK_COMMENT);
        let text = "/* TODO with /**/ empty nested */\nlet x = 1;";
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "with /**/ empty nested".to_string()));
    }

    // Multi-line string tests
    const TEST_WITH_MULTI_LINE_STRING: [SyntaxRule; 2] = [
        SyntaxRule::LineComment(b"//"),
        SyntaxRule::SkipDelimited(b"`", b"`"),
    ];

    #[test]
    fn multi_line_string_todo_inside_ignored() {
        let parser = Parser::new("TODO", &TEST_WITH_MULTI_LINE_STRING);
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
        let parser = Parser::new("TODO", &TEST_WITH_MULTI_LINE_STRING);
        let text = r#"let x = `// TODO fake`;
// TODO real
let y = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (2, 2, "real".to_string()));
    }

    #[test]
    fn multi_line_string_todo_after_detected() {
        let parser = Parser::new("TODO", &TEST_WITH_MULTI_LINE_STRING);
        let text = r#"let x = `raw string content`;
// TODO after raw string
let y = 1;"#;
        let todos = parser.parse_todos(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (2, 2, "after raw string".to_string()));
    }

    #[test]
    fn multi_line_string_delimiter_in_comment() {
        let parser = Parser::new("TODO", &TEST_WITH_MULTI_LINE_STRING);
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
        SyntaxRule::LineComment(b"//"),
        SyntaxRule::BlockComment(b"/*", b"*/"),
    ];

    #[test]
    fn mixed_comment_styles_both_detected() {
        let parser = Parser::new("TODO", &TEST_MIXED_COMMENTS);
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
        let parser = Parser::new("TODO", &TEST_MIXED_COMMENTS);
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
