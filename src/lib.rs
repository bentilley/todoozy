pub mod error;
mod fs;
mod lang;
pub mod todo;

#[cfg(feature = "testutils")]
pub mod testutils;

pub use fs::FileType;
pub use todo::{Todo, TodoIdentifier, Todos};

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
// For display, references roll up into the primary:
// - Reference title becomes a `## Subtitle` in description
// - Reference description appended after subtitle
// - Tags/metadata merged for display (kept separate in model)
// - Locations list shows all, with `*` marking the primary
//
// These warnings indicate ID assignment issues - see separate TODO for
// improved branch-aware ID assignment system.

fn parse_files(files: fs::Walk) -> Result<todo::Todos> {
    let todos: Arc<Mutex<Vec<Todo>>> = Arc::new(Mutex::new(Vec::new()));

    files.run(|| {
        let todos = Arc::clone(&todos);
        move |path: &std::path::Path| {
            if let Some(file_path) = path.to_str() {
                if let Some(ref mut tdz) = parse_file(file_path) {
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

pub fn parse_file(file_path: &str) -> Option<Vec<todo::Todo>> {
    let text = match std::fs::read_to_string(file_path) {
        Ok(text) => text,
        Err(err) => match err.kind() {
            std::io::ErrorKind::InvalidData => return None,
            _ => panic!("Unable to read file ({}): {}", file_path, err),
        },
    };

    Some(
        parse_text(&text, crate::fs::get_filetype(file_path)?)?
            .into_iter()
            .map(|mut todo| {
                todo.location.file_path = Some(file_path.to_string());
                todo
            })
            .collect(),
    )
}

pub const TODO_TOKEN: &'static str = "TODO";

pub fn parse_text(text: &str, file_type: crate::fs::FileType) -> Option<Vec<Todo>> {
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

    Some(
        raw_todos
            .into_iter()
            .filter_map(|raw| match Todo::try_from(raw) {
                Ok(todo) => Some(todo),
                Err(err) => {
                    eprintln!("Error: {}", err);
                    None
                }
            })
            .collect(),
    )
}
