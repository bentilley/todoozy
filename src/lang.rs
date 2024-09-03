use crate::constants::TODOOZY_DELIMITER;

pub mod go;
pub mod python;
pub mod rust;
pub mod tdz;

pub enum SyntaxRule<'a> {
    LineComment(&'a str),
    BlockComment(&'a str, &'a str),
    // String(&'a [u8]),
}

struct CommentFormat<T> {
    token: T,
    todo_token: String,
}

pub struct Parser {
    line_comment_delimiters: Vec<CommentFormat<String>>,
    block_comment_delimiters: Vec<CommentFormat<(String, String)>>,
}

impl Parser {
    fn new(syntax_rules: &[SyntaxRule]) -> Self {
        let mut line_comment_delimiters = Vec::new();
        let mut block_comment_delimiters = Vec::new();

        for rule in syntax_rules {
            match rule {
                SyntaxRule::LineComment(token) => {
                    line_comment_delimiters.push(CommentFormat {
                        token: token.to_string(),
                        todo_token: format!("{} {}", token, TODOOZY_DELIMITER),
                    });
                }
                SyntaxRule::BlockComment(start, end) => {
                    block_comment_delimiters.push(CommentFormat {
                        token: (start.to_string(), end.to_string()),
                        todo_token: format!("{} {}", start, TODOOZY_DELIMITER),
                    });
                }
            }
        }

        Self {
            line_comment_delimiters,
            block_comment_delimiters,
        }
    }

    fn parse_todos(&self, text: &str) -> Vec<(usize, usize, String)> {
        let mut todos = Vec::<(usize, usize, String)>::new();

        let mut lines = text.lines().enumerate().peekable();
        while let Some((i, line)) = lines.next() {
            let trimmed = line.trim_start();

            for line_comment_delimiter in &self.line_comment_delimiters {
                if trimmed.starts_with(&line_comment_delimiter.todo_token) {
                    let mut todo: Vec<String> = Vec::new();

                    let v: Vec<&str> = line.split(TODOOZY_DELIMITER).collect();
                    todo.push(v[1].trim().to_string());
                    let prefix = v[0].len();

                    while let Some((j, line)) = lines.peek() {
                        if !line.trim_start().starts_with(&line_comment_delimiter.token) {
                            todos.push((i + 1, *j, todo.join("\n")));
                            break;
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

                    let v: Vec<&str> = line.split(TODOOZY_DELIMITER).collect();
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
                            break;
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
