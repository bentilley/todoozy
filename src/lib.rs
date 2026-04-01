pub mod error;
mod fs;
mod lang;
pub mod todo;

#[cfg(feature = "testutils")]
pub mod testutils;

pub use fs::FileType;
pub use todo::{parser::TodoParser, Todo, TodoIdentifier, Todos};

use error::Result;
use std::sync::{Arc, Mutex};

// TODO #64 (D) 2026-03-22 VCS interface for extracting todo history +vcs
//
// Abstract the VCS backend (git for now) to extract todo lifecycle data:
// - Created date: when the commit adding the TODO was merged
// - Completed date: when the commit removing the TODO was merged
// - Author: who added the TODO
//
// This makes VCS the source of truth for dates rather than in-comment fields
// which can be spoofed and duplicate what VCS already tracks.
//
// Design as an interface/trait so other VCS backends (hg, svn, etc.) can be
// supported in the future:
//
//   trait VcsBackend {
//       fn get_todo_created(&self, file: &str, line: u32, id: u32) -> Option<DateTime>;
//       fn get_todo_removed(&self, id: u32) -> Option<DateTime>;
//       fn get_all_historical_ids(&self) -> Vec<u32>;  // for cache build
//   }
//
// The git implementation would use git log/blame to find relevant commits.

/// Search for all the available todos in the project.
///
/// * `exclude`: A slice of files to exclude from the search.
pub fn get_todos(exclude: &[String]) -> Result<todo::Todos> {
    let walk = fs::Walk::new(&fs::WalkConfig::new(".", Some(exclude)));
    let todos = parse_files(walk)?;
    Ok(todos.link_references())
}

pub fn get_todo(id: u32, exclude: &[String]) -> Result<Option<Todo>> {
    let todos = get_todos(exclude)?;
    Ok(todos
        .into_iter()
        .find(|t| t.id == Some(TodoIdentifier::Primary(id))))
}

fn parse_files(files: fs::Walk) -> Result<todo::Todos> {
    let todos: Arc<Mutex<Vec<Todo>>> = Arc::new(Mutex::new(Vec::new()));

    files.run(|| {
        let todos = Arc::clone(&todos);
        move |path: &std::path::Path| {
            if let Some(file_path) = path.to_str() {
                if let Ok(ref mut tdz) = parse_file(file_path) {
                    todos.lock().unwrap().append(tdz);
                }
            }
        }
    });

    let todos = Arc::try_unwrap(todos)
        .expect("Walk should have completed")
        .into_inner()
        .unwrap();
    Ok(todos.into())
}

pub const TODO_TOKEN: &'static str = "TODO";

pub fn parse_file(file_path: &str) -> Result<Vec<todo::Todo>> {
    let file_type = crate::fs::get_filetype(file_path).ok_or("Invalid file type")?;

    let text = match std::fs::read_to_string(file_path) {
        Ok(text) => text,
        Err(_) => return Err(format!("Failed to read file: {}", file_path).into()),
    };

    Ok(parse_text(&text, file_type)
        .into_iter()
        .map(|mut todo| {
            todo.location.file_path = Some(file_path.to_string());
            todo
        })
        .collect())
}

pub fn parse_text(text: &str, file_type: crate::fs::FileType) -> Vec<Todo> {
    TodoParser::new(TODO_TOKEN).parse_text(text, file_type)
}
