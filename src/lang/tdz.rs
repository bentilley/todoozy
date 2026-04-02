use crate::lang::RawTodo;

pub struct Parser {
    todo_token: String,
}

impl Parser {
    pub fn new(todo_token: &str) -> Self {
        Self {
            todo_token: todo_token.to_string(),
        }
    }

    /// Check if position is at a `# TODO ` marker at the start of a line.
    /// Returns false for `## TODO` or `### TODO` (reserved for sub-tasks).
    fn is_todo_marker_at(bytes: &[u8], position: usize, todo_marker: &[u8]) -> bool {
        // Check for `# TODO ` but NOT `## TODO` (would start with ##)
        if position > 0 && bytes[position - 1] == b'#' {
            return false;
        }
        bytes[position..].starts_with(todo_marker)
    }

    /// Find where the current TODO's content ends.
    /// Returns (content_end, end_line, next_position, next_line_number)
    fn find_todo_end(
        bytes: &[u8],
        start: usize,
        start_line: usize,
        todo_marker: &[u8],
    ) -> (usize, usize, usize, usize) {
        let len = bytes.len();
        let mut position = start;
        let mut line_number = start_line;

        // Skip to end of first line (title line)
        while position < len && bytes[position] != b'\n' {
            position += 1;
        }
        let mut last_content_end = position;
        let mut last_content_line = line_number;

        if position < len {
            position += 1; // skip newline
            line_number += 1;
        }

        // Scan remaining lines
        while position < len {
            let line_start = position;

            // Check if this line starts a new TODO
            if Self::is_todo_marker_at(bytes, position, todo_marker) {
                // End of current TODO, return position before this line
                return (last_content_end, last_content_line, line_start, line_number);
            }

            // Find end of this line
            while position < len && bytes[position] != b'\n' {
                position += 1;
            }

            // Track last line with content (for trimming trailing blank lines)
            let line_content = &bytes[line_start..position];
            if !line_content.iter().all(|&b| b == b' ' || b == b'\t') {
                last_content_end = position;
                last_content_line = line_number;
            }

            if position < len {
                position += 1; // skip newline
                line_number += 1;
            }
        }

        // Reached EOF
        (last_content_end, last_content_line, position, line_number)
    }
}

impl super::RawParser for Parser {
    /// Parse TODOs from a .tdz file.
    ///
    /// Format:
    /// ```text
    /// # TODO (A) Title here +tag key:value
    ///
    /// Description spans multiple lines until next `# TODO` or EOF.
    ///
    /// # TODO (B) Another todo
    ///
    /// Its description here.
    /// ```
    ///
    /// Rules:
    /// - `# TODO ` starts a new TODO (must have space after TODO)
    /// - Full syntax supported: #id, (priority), dates, +tags, key:value
    /// - `## TODO` / `### TODO` NOT parsed (reserved for future sub-tasks)
    /// - Trailing whitespace trimmed
    fn parse(&self, text: &[u8]) -> Vec<RawTodo> {
        let todo_marker_str = format!("# {} ", self.todo_token);
        let todo_marker = todo_marker_str.as_bytes();
        let len = text.len();
        let mut todos = Vec::new();

        let mut position = 0;
        let mut line_number = 1;

        while position < len {
            // Check if we're at a TODO marker at start of line
            if Self::is_todo_marker_at(text, position, todo_marker) {
                let start_line = line_number;
                let content_start = position + todo_marker.len();

                // Find the end of this TODO (next `# TODO ` at line start or EOF)
                let (content_end, end_line, next_position, next_line) =
                    Self::find_todo_end(text, content_start, line_number, todo_marker);

                // Extract and trim the content (trim each line's trailing whitespace)
                let raw_content =
                    std::str::from_utf8(&text[content_start..content_end]).unwrap_or("");
                let content = raw_content
                    .lines()
                    .map(|line| line.trim_end())
                    .collect::<Vec<_>>()
                    .join("\n")
                    .trim()
                    .to_string();

                todos.push((start_line, end_line, content));

                position = next_position;
                line_number = next_line;
            } else {
                // Advance to next line
                while position < len && text[position] != b'\n' {
                    position += 1;
                }
                if position < len {
                    position += 1; // skip newline
                    line_number += 1;
                }
            }
        }

        todos
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::RawParser;

    #[test]
    fn single_todo_title_only() {
        let text = "# TODO Simple task";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "Simple task".to_string()));
    }

    #[test]
    fn single_todo_with_description() {
        let text = "# TODO Task title\n\nThis is the description.\nSecond line of description.";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                1,
                4,
                "Task title\n\nThis is the description.\nSecond line of description.".to_string()
            )
        );
    }

    #[test]
    fn full_syntax() {
        let text = "# TODO #42 (A) 2026-03-22 Task title +tag key:value\n\nDescription here.";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                1,
                3,
                "#42 (A) 2026-03-22 Task title +tag key:value\n\nDescription here.".to_string()
            )
        );
    }

    #[test]
    fn multiple_todos() {
        let text =
            "# TODO First task\n\nDescription one.\n\n# TODO Second task\n\nDescription two.";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(
            todos[0],
            (1, 3, "First task\n\nDescription one.".to_string())
        );
        assert_eq!(
            todos[1],
            (5, 7, "Second task\n\nDescription two.".to_string())
        );
    }

    #[test]
    fn back_to_back_todos_no_description() {
        let text = "# TODO First\n# TODO Second\n# TODO Third";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 3);
        assert_eq!(todos[0], (1, 1, "First".to_string()));
        assert_eq!(todos[1], (2, 2, "Second".to_string()));
        assert_eq!(todos[2], (3, 3, "Third".to_string()));
    }

    #[test]
    fn trailing_whitespace_trimmed() {
        let text = "# TODO Task with trailing space   \n\nDescription.   \n\n\n";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (1, 3, "Task with trailing space\n\nDescription.".to_string())
        );
    }

    #[test]
    fn eof_without_trailing_newline() {
        let text = "# TODO Task at EOF";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (1, 1, "Task at EOF".to_string()));
    }

    #[test]
    fn h2_todo_not_parsed() {
        let text = "# TODO Real task\n\n## TODO This is a heading, not a todo";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                1,
                3,
                "Real task\n\n## TODO This is a heading, not a todo".to_string()
            )
        );
    }

    #[test]
    fn h3_todo_not_parsed() {
        let text = "# TODO Real task\n\n### TODO Also not a todo";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (1, 3, "Real task\n\n### TODO Also not a todo".to_string())
        );
    }

    #[test]
    fn empty_file_returns_empty_vec() {
        let text = "";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 0);
    }

    #[test]
    fn todolist_not_matched() {
        let text = "# TODOLIST is not a todo";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 0);
    }

    #[test]
    fn todo_mid_line_not_matched() {
        let text = "Some text # TODO not at start\n# TODO Real todo";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (2, 2, "Real todo".to_string()));
    }

    #[test]
    fn file_with_no_todos() {
        let text = "Just some regular text.\nNo TODOs here.\n## Heading";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 0);
    }

    #[test]
    fn todo_with_code_block_in_description() {
        let text = "# TODO Add tests\n\n```rust\nfn example() {}\n```";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(
            todos[0],
            (
                1,
                5,
                "Add tests\n\n```rust\nfn example() {}\n```".to_string()
            )
        );
    }

    #[test]
    fn multiple_blank_lines_between_todos() {
        let text = "# TODO First\n\n\n\n# TODO Second";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0], (1, 1, "First".to_string()));
        assert_eq!(todos[1], (5, 5, "Second".to_string()));
    }

    #[test]
    fn todo_without_space_after_todo_not_matched() {
        let text = "# TODOthing\n# TODO Real todo";
        let todos = Parser::new("TODO").parse_str(text);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0], (2, 2, "Real todo".to_string()));
    }
}
