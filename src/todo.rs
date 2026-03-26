pub mod filter;
pub mod parser;
pub mod sort;

use std::fs::File;
use std::io::{self, prelude::*, BufReader, BufWriter};

use derive_builder::Builder;
use tempfile::NamedTempFile;

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum TodoIdentifier {
    Primary(u32),
    Reference(u32),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Metadata(HashMap<String, String>);

// TODO #20 (E) 2024-09-17 Vec metadata keys +improvement
//
// Currently, repeated metadata keys are not allowed. This means that if a todo is parsed with the
// same metadata key multiple times, we error the parsing.
//
// There might be valid cases when a specific key lends itself to having multiple values associated
// with the same key (i.e. a list/vector metadata type). This needs better understanding and
// defining before implementation.
//
// Note: This would also enable DIY dependency tracking via metadata, e.g.:
//   # TODO #43 Implement auth `depends:42` `depends:41`
// Without first-class dependency support (deemed too complex for now), users who
// want dependencies can use array metadata to roll their own.
impl Metadata {
    pub fn new() -> Self {
        Metadata(HashMap::new())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_str())
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        match self.get(key) {
            Some(_) => {
                return Err(format!("Key {} already exists", key));
            }
            None => {
                self.0.insert(key.to_string(), value.to_string());
            }
        };
        Ok(())
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, String, String> {
        self.0.iter()
    }
}

// TODO #68 (E) 2026-03-22 Deprecate in-comment created/completed dates +model +vcs
//
// With VCS integration providing accurate created/completed dates from git
// history, the in-comment date fields become redundant:
//
//   // TODO #43 (A) 2024-08-05 Fix bug   <- 2024-08-05 is duplicating git info
//
// Options:
// 1. Remove date parsing entirely (breaking change)
// 2. Keep parsing but ignore in favor of VCS dates (silent deprecation)
// 3. Keep as optional override (explicit > inferred)
//
// Recommendation: Option 2 initially, then Option 1 in a future major version.
// Display VCS dates in UI/CLI, but don't break existing TODOs that have dates.

// TODO #69 (E) 2026-03-22 Consolidate +project and @context into +tag +model
//
// The distinction between +project and @context doesn't add much value in a
// repo-specific context. @context was inherited from todo.txt where it made
// sense for life-wide task lists ("@home", "@work"), but in a codebase the
// context is always "this repo."
//
// Proposal: Simplify to two primitives:
// - `+tag` (boolean tag, replaces both +project and @context)
// - `key:value` (arbitrary string metadata)
//
// This is a breaking change. Migration path:
// 1. Parse both syntaxes, normalize to tags internally
// 2. Deprecation warnings for @context usage
// 3. Future version removes @context parsing
//
// Leaves room to repurpose @syntax later if a compelling use case emerges
// (e.g., @status was considered but deferred - metadata handles it fine).

impl FromIterator<(std::string::String, std::string::String)> for Metadata {
    fn from_iter<I: IntoIterator<Item = (std::string::String, std::string::String)>>(
        iter: I,
    ) -> Self {
        let mut metadata = Metadata::new();
        for (key, value) in iter {
            match metadata.set(&key, &value) {
                Err(e) => panic!("{}", e),
                Ok(_) => {}
            };
        }
        metadata
    }
}

#[derive(Builder, Debug, Default, PartialEq)]
pub struct Todo {
    #[builder(default)]
    pub id: Option<TodoIdentifier>,

    #[builder(default)]
    pub file: Option<String>,
    #[builder(default)]
    pub line_number: Option<usize>,
    #[builder(default)]
    pub end_line_number: Option<usize>,

    #[builder(default)]
    pub priority: Option<char>,
    #[builder(default)]
    pub completion_date: Option<chrono::NaiveDate>,
    #[builder(default)]
    pub creation_date: Option<chrono::NaiveDate>,

    #[builder(default)]
    pub title: String,
    #[builder(default)]
    pub description: Option<String>,

    #[builder(default)]
    pub projects: Vec<String>,
    #[builder(default)]
    pub contexts: Vec<String>,

    #[builder(default)]
    pub metadata: Metadata,
}

impl Todo {
    pub fn display_id(&self) -> String {
        match &self.id {
            Some(TodoIdentifier::Primary(id)) => format!("#{}", id),
            Some(TodoIdentifier::Reference(id)) => format!("&{}", id),
            None => "#-".to_string(),
        }
    }

    pub fn display_priority(&self) -> String {
        match self.priority {
            Some(priority) => format!("({})", priority),
            None => "(Z)".to_string(),
        }
    }

    pub fn display_location_start(&self) -> String {
        match (&self.file, self.line_number) {
            (Some(file), Some(line)) => {
                format!("{}:{}", file, line)
            }
            (Some(file), None) => file.to_string(),
            _ => String::new(),
        }
    }

    pub fn display_title(&self) -> String {
        let projects: String = self
            .projects
            .iter()
            .map(|p| format!("+{}", p))
            .collect::<Vec<_>>()
            .join(" ");

        let contexts: String = self
            .contexts
            .iter()
            .map(|c| format!("@{}", c))
            .collect::<Vec<_>>()
            .join(" ");

        format!("{} {} {}", self.title, projects, contexts)
    }

    pub fn has_project(&self, project: &str) -> bool {
        self.projects.iter().any(|p| p == project)
    }

    pub fn has_context(&self, context: &str) -> bool {
        self.contexts.iter().any(|c| c == context)
    }

    pub fn write_id(&self) -> Result<(), Box<dyn std::error::Error>> {
        let id = match &self.id {
            Some(TodoIdentifier::Primary(id)) => *id,
            Some(TodoIdentifier::Reference(_)) => {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Cannot write ID for a reference todo",
                )))
            }
            None => return Err(Box::new(io::Error::new(io::ErrorKind::NotFound, "No ID"))),
        };
        let file_name = match self.file {
            Some(ref file) => file,
            None => return Err(Box::new(io::Error::new(io::ErrorKind::NotFound, "No file"))),
        };
        let line_number = match self.line_number {
            Some(line_number) => line_number,
            None => {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::NotFound,
                    "No line number",
                )))
            }
        };

        let file = File::open(file_name)?;
        let reader = BufReader::new(file);

        let tmp_file = NamedTempFile::new()?;
        let mut writer = BufWriter::new(tmp_file.as_file());

        for (i, line) in reader.lines().enumerate() {
            match line {
                Ok(line) => {
                    if i + 1 == line_number {
                        let new_line = match line.split_once("TODO") {
                            Some((pref, suff)) => {
                                format!("{}TODO #{}{}", pref, id, suff)
                            }
                            None => {
                                return Err(Box::new(io::Error::new(
                                    io::ErrorKind::NotFound,
                                    "No TODO",
                                )))
                            }
                        };
                        writer.write_all(new_line.as_bytes())?;
                        writer.write_all(b"\n")?;
                    } else {
                        writer.write_all(line.as_bytes())?;
                        writer.write_all(b"\n")?;
                    }
                }
                Err(e) => return Err(Box::new(e)),
            }
        }

        writer.flush()?;
        std::fs::copy(tmp_file.path(), file_name)?;

        Ok(())
    }
}

pub struct Todos(Vec<Todo>);

impl Todos {
    pub fn get_max_id(&self) -> u32 {
        self.0
            .iter()
            .filter_map(|t| match &t.id {
                Some(TodoIdentifier::Primary(id)) => Some(*id),
                _ => None, // Don't count references or None
            })
            .max()
            .unwrap_or(0)
    }

    pub fn apply_filter<F>(&mut self, filter: F)
    where
        F: Fn(&Todo) -> bool,
    {
        self.0.retain(filter);
    }

    pub fn apply_sort<F>(&mut self, sorter: F)
    where
        F: Fn(&Todo, &Todo) -> std::cmp::Ordering,
    {
        self.0.sort_by(sorter);
    }
}

impl std::ops::Deref for Todos {
    type Target = Vec<Todo>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<Todo>> for Todos {
    fn from(todos: Vec<Todo>) -> Self {
        Todos(todos)
    }
}

impl From<Todos> for Vec<Todo> {
    fn from(todos: Todos) -> Self {
        todos.0
    }
}

impl IntoIterator for Todos {
    type Item = Todo;
    type IntoIter = std::vec::IntoIter<Todo>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[test]
fn test_todos() {
    let todos: Todos = vec![
        TodoBuilder::default()
            .id(Some(TodoIdentifier::Primary(1)))
            .build()
            .unwrap(),
        TodoBuilder::default()
            .id(Some(TodoIdentifier::Primary(2)))
            .build()
            .unwrap(),
    ]
    .into();
    assert_eq!(todos.get_max_id(), 2);
}
