mod constants;
pub mod filter;
pub mod fs;
mod lang;
mod parse;
pub mod sort;
mod todo;

pub use todo::Todo;

use ignore::Walk;
use std::error;

/// Search for all the available todos in the project.
///
/// * `exclude`: A slice of files to exclude from the search.
pub fn get_todos(exclude: &[String]) -> Result<Vec<todo::Todo>, Box<dyn error::Error>> {
    parse_files(fs::get_files(exclude))
}

fn parse_files(files: Walk) -> Result<Vec<todo::Todo>, Box<dyn error::Error>> {
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

    Ok(todos)
}

type RawTodo = (usize, usize, String);

// TODO (E) 2024-09-02 Add more language support +improvement
//
// Candidates:
//   - terraform (.tf)
//   - yaml (.yaml, .yml)
//   - Dockerfile
//   - Makefile
//   - Markdown? (.md)
//   - Protobuf? (.proto)
fn parse_file(file_path: &str) -> Option<Vec<todo::Todo>> {
    let text = std::fs::read_to_string(file_path).expect("Unable to read file");

    use crate::fs::FileType;
    let raw_todos = match crate::fs::get_filetype(file_path) {
        Some(FileType::Go) => Some(lang::go::extract_todos(&text)),
        Some(FileType::Python) => Some(lang::python::extract_todos(&text)),
        Some(FileType::Rust) => Some(lang::rust::extract_todos(&text)),
        Some(FileType::Todoozy) => Some(lang::tdz::extract_todos(&text)),
        _ => None,
    };

    if raw_todos.is_none() {
        return None;
    }
    Some(parse_raw(raw_todos.unwrap(), file_path))
}

fn parse_raw(raw_todos: Vec<RawTodo>, file_path: &str) -> Vec<todo::Todo> {
    let mut todos = Vec::<todo::Todo>::new();
    for (start, end, raw) in raw_todos {
        match parse::todo(&raw) {
            Ok((_, mut t)) => {
                t.file = Some(file_path.to_owned());
                t.line_number = Some(start as usize);
                t.end_line_number = Some(end as usize);
                todos.push(t)
            }
            Err(err) => eprintln!("Error: {}", err),
        }
    }
    todos
}
