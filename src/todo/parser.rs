use super::Todo;
use crate::lang::RawParser;

pub struct TodoParser {
    todo_token: String,
}

impl TodoParser {
    pub fn new(todo_token: &str) -> Self {
        Self {
            todo_token: todo_token.to_string(),
        }
    }

    fn get_parser_for_file_type(&self, file_type: crate::fs::FileType) -> Box<dyn RawParser + '_> {
        use crate::fs::FileType::*;
        use crate::lang::*;
        match file_type {
            Bash | Ksh | Sh | Zsh => Box::new(Parser::new(&self.todo_token, &sh::SH)),
            C => Box::new(Parser::new(&self.todo_token, &c::C)),
            Cpp => Box::new(Parser::new(&self.todo_token, &cpp::CPP)),
            Dockerfile => Box::new(Parser::new(&self.todo_token, &dockerfile::DOCKERFILE)),
            Go => Box::new(Parser::new(&self.todo_token, &go::GO)),
            Html => Box::new(Parser::new(&self.todo_token, &html::HTML)),
            JavaScript => Box::new(Parser::new(&self.todo_token, &javascript::JAVASCRIPT)),
            Makefile => Box::new(Parser::new(&self.todo_token, &makefile::MAKEFILE)),
            Markdown => Box::new(Parser::new(&self.todo_token, &markdown::MARKDOWN)),
            Protobuf => Box::new(Parser::new(&self.todo_token, &protobuf::PROTOBUF)),
            Python => Box::new(Parser::new(&self.todo_token, &python::PYTHON)),
            Rust => Box::new(Parser::new(&self.todo_token, &rust::RUST)),
            Sql => Box::new(Parser::new(&self.todo_token, &sql::SQL)),
            Terraform => Box::new(Parser::new(&self.todo_token, &terraform::TERRAFORM)),
            Todoozy => Box::new(tdz::Parser::new(&self.todo_token)),
            Typescript => Box::new(Parser::new(&self.todo_token, &javascript::JAVASCRIPT)),
            YAML => Box::new(Parser::new(&self.todo_token, &yaml::YAML)),
        }
    }

    pub fn parse_text(&self, text: &str, file_type: crate::fs::FileType) -> Vec<Todo> {
        let parser = self.get_parser_for_file_type(file_type);
        parser
            .parse_str(&text)
            .into_iter()
            .filter_map(|raw| match Todo::try_from(raw) {
                Ok(todo) => Some(todo),
                Err(err) => {
                    eprintln!("Error: {}", err);
                    None
                }
            })
            .collect()
    }

    pub fn parse_bytes(&self, bytes: &[u8], file_type: crate::fs::FileType) -> Vec<Todo> {
        let parser = self.get_parser_for_file_type(file_type);
        parser
            .parse(&bytes)
            .into_iter()
            .filter_map(|raw| match Todo::try_from(raw) {
                Ok(todo) => Some(todo),
                Err(err) => {
                    eprintln!("Error: {}", err);
                    None
                }
            })
            .collect()
    }
}
