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
                todos.append(&mut parse_file(file_path));
            }
            Err(err) => eprintln!("Error: {}", err),
        }
    }

    Ok(todos)
}

// TODO (C) 2024-09-02 Add more language support +improvement
//
// Not sure what else we'll need, but there'll def be others!
fn parse_file(file_path: &str) -> Vec<todo::Todo> {
    match get_extension_from_filename(file_path) {
        Some("go") => parse_raw(lang::go::extract_todos(file_path), file_path),
        Some("py") => parse_raw(lang::python::extract_todos(file_path), file_path),
        Some("rs") => parse_raw(lang::rust::extract_todos(file_path), file_path),
        Some("tdz") => parse_raw(lang::tdz::extract_todos(file_path), file_path),
        _ => {
            // eprintln!("[{}]: Unsupported file type", file_path);
            Vec::new()
        }
    }
}

fn get_extension_from_filename(filename: &str) -> Option<&str> {
    if filename.ends_with(".tdz") {
        return Some("tdz");
    }
    std::path::Path::new(filename)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
}

#[test]
fn test_get_extension_from_filename() {
    assert_eq!(get_extension_from_filename("dir/test.tdz"), Some("tdz"));
    assert_eq!(get_extension_from_filename("test.tdz"), Some("tdz"));
    assert_eq!(get_extension_from_filename("dir/.tdz"), Some("tdz"));
    assert_eq!(get_extension_from_filename("./.tdz"), Some("tdz"));
    assert_eq!(get_extension_from_filename(".tdz"), Some("tdz"));
    assert_eq!(get_extension_from_filename("test.rs"), Some("rs"));
    assert_eq!(get_extension_from_filename("test"), None);
}

fn parse_raw(raw_todos: Vec<(usize, usize, String)>, file_path: &str) -> Vec<todo::Todo> {
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
