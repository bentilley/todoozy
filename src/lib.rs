mod fs;
mod lang;
pub mod todo;

#[cfg(feature = "testutils")]
pub mod testutils;

pub use fs::FileType;
pub use todo::{Todo, Todos};

use ignore::Walk;
use std::error;

/// Search for all the available todos in the project.
///
/// * `exclude`: A slice of files to exclude from the search.
pub fn get_todos(exclude: &[String]) -> Result<todo::Todos, Box<dyn error::Error>> {
    parse_files(fs::get_files(exclude))
}

// TODO #61 (D) 2026-03-22 Link references to primaries and validate IDs +refs
//
// After parsing all files, link TodoRef instances to their primary Todo:
// 1. Build a map of id -> Todo for all primaries
// 2. For each TodoRef, find its primary and add to `references` vec
// 3. Validation with warnings (don't block, just warn):
//    - Orphan reference (no primary found): "Warning: TODO &43 references
//      non-existent primary #43 at `file:line`"
//    - Duplicate primary (same ID twice): "Warning: Duplicate TODO #43 found
//      at `file:line`, ignoring (first occurrence at `file:line`)"
//
// These warnings indicate ID assignment issues - see separate TODO for
// improved branch-aware ID assignment system.

fn parse_files(files: Walk) -> Result<todo::Todos, Box<dyn error::Error>> {
    let mut todos = Vec::<todo::Todo>::new();

    for file in files {
        match file {
            Ok(entry) => {
                if entry.file_type().unwrap().is_dir() {
                    continue;
                }

                let file_path = entry.path().to_str().unwrap();
                if let Some(ref mut tdz) = parse_file(file_path) {
                    todos.append(tdz);
                }
            }
            Err(err) => eprintln!("Error: {}", err),
        }
    }

    Ok(todo::Todos(todos))
}

type RawTodo = (usize, usize, String);

pub fn parse_file(file_path: &str) -> Option<Vec<todo::Todo>> {
    let text = match std::fs::read_to_string(file_path) {
        Ok(text) => text,
        Err(err) => match err.kind() {
            std::io::ErrorKind::InvalidData => return None,
            _ => panic!("Unable to read file ({}): {}", file_path, err),
        },
    };

    parse_text(
        &text,
        crate::fs::get_filetype(file_path)?,
        Some(file_path.to_owned()),
    )
}

pub const TODO_TOKEN: &'static str = "TODO";

pub fn parse_text(
    text: &str,
    file_type: crate::fs::FileType,
    file_path: Option<String>,
) -> Option<Vec<Todo>> {
    use crate::fs::FileType;
    let syntax_rules: &[lang::SyntaxRule] = match file_type {
        FileType::Bash | FileType::Ksh | FileType::Sh | FileType::Zsh => &lang::sh::SH,
        FileType::Dockerfile => &lang::dockerfile::DOCKERFILE,
        FileType::Go => &lang::go::GO,
        FileType::Makefile => &lang::makefile::MAKEFILE,
        FileType::Markdown => &lang::markdown::MARKDOWN,
        FileType::Protobuf => &lang::protobuf::PROTOBUF,
        FileType::Python => &lang::python::PYTHON,
        FileType::Rust => &lang::rust::RUST,
        FileType::Terraform => &lang::terraform::TERRAFORM,
        FileType::Todoozy => return None, // see src/lang/tdz.rs for implementation TODO
        FileType::Typescript => &lang::typescript::TYPESCRIPT,
        FileType::YAML => &lang::yaml::YAML,
    };
    let parser = lang::Parser::new(TODO_TOKEN, &syntax_rules);
    let raw_todos = parser.parse_todos(&text);
    if raw_todos.len() == 0 {
        return None;
    }
    Some(parse_raw(raw_todos, file_path))
}

fn parse_raw(raw_todos: Vec<RawTodo>, file_path: Option<String>) -> Vec<todo::Todo> {
    let mut todos = Vec::<todo::Todo>::new();
    for (start, end, raw) in raw_todos {
        match todo::parser::todo(&raw) {
            Ok((_, mut t)) => {
                t.file = file_path.clone();
                t.line_number = Some(start as usize);
                t.end_line_number = Some(end as usize);
                todos.push(t)
            }
            Err(err) => eprintln!("Error: {}", err),
        }
    }
    todos
}
