pub mod filter;
pub mod parser;
pub mod sort;

use std::fmt;
use std::fs::File;
use std::io::{self, prelude::*, BufReader, BufWriter};

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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Location {
    pub file_path: Option<String>,
    pub start_line_num: usize,
    pub end_line_num: usize,
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let file_display = self.file_location_prefix();
        if self.start_line_num == self.end_line_num {
            return write!(f, "{}{}", file_display, self.start_line_num);
        }
        write!(
            f,
            "{}{}-{}",
            file_display, self.start_line_num, self.end_line_num
        )
    }
}

impl Location {
    pub fn new(file: Option<String>, line_number: usize, end_line_number: usize) -> Self {
        Location {
            file_path: file,
            start_line_num: line_number,
            end_line_num: end_line_number,
        }
    }

    fn file_location_prefix(&self) -> String {
        self.file_path
            .clone()
            .map_or("".to_string(), |p| format!("{}:", p))
    }

    pub fn from_file_line(file: Option<String>, line_number: usize) -> Self {
        Location {
            file_path: file,
            start_line_num: line_number,
            end_line_num: line_number,
        }
    }

    pub fn display_start(&self) -> String {
        format!("{}{}", self.file_location_prefix(), self.start_line_num)
    }
}

// TODO #76 (A) 2026-03-30 Consolidate TUI logic with the new logic in the CLI
//
// We have duplicated logic the TUI code and the CLI code. Needs to be lifted up to here and just
// called from both.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Todo {
    pub id: Option<TodoIdentifier>,

    pub priority: Option<char>,
    pub completion_date: Option<chrono::NaiveDate>,
    pub creation_date: Option<chrono::NaiveDate>,

    pub title: String,
    pub description: Option<String>,

    pub tags: Vec<String>,
    pub metadata: Metadata,

    pub location: Location,
    pub references: Vec<Todo>,
}

impl TryFrom<crate::lang::RawTodo> for Todo {
    type Error = String;

    fn try_from(raw_todo: crate::lang::RawTodo) -> Result<Self, Self::Error> {
        let (start, end, text) = raw_todo;
        let location = Location::new(None, start, end);
        let info = parser::TodoInfo::try_from(text.as_str()).map_err(|e| format!("{}", e))?;
        Ok(Todo::new(info, location))
    }
}

impl Todo {
    pub fn new(info: parser::TodoInfo, location: Location) -> Self {
        Todo {
            id: info.id,
            priority: info.priority,
            completion_date: info.completion_date,
            creation_date: info.creation_date,
            title: info.title,
            description: info.description,
            tags: info.tags,
            metadata: info.metadata,
            location,
            references: Vec::new(),
        }
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

        let file_path = self
            .location
            .file_path
            .as_ref()
            .ok_or("Cannot write ID: No file path in location")?;
        let file = File::open(&file_path)?;
        let reader = BufReader::new(file);

        let tmp_file = NamedTempFile::new()?;
        let mut writer = BufWriter::new(tmp_file.as_file());

        for (i, line) in reader.lines().enumerate() {
            match line {
                Ok(line) => {
                    if i + 1 == self.location.start_line_num {
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
        std::fs::copy(tmp_file.path(), &file_path)?;

        Ok(())
    }

    pub fn display_locations_with_marker(&self) -> Vec<String> {
        let mut locations = Vec::new();

        locations.push(format!("* {}", self.location)); // primary marker

        for reference in &self.references {
            locations.push(format!("  {}", reference.location));
        }

        locations
    }

    /// Description with reference titles as ## Subtitles
    pub fn display_merged_description(&self) -> Option<String> {
        let mut parts = Vec::new();

        // Add primary description if present
        if let Some(ref desc) = self.description {
            parts.push(desc.clone());
        }

        // Add each reference as a subtitle section
        for reference in &self.references {
            let subtitle = format!("## {}", reference.title);
            parts.push(subtitle);

            if let Some(ref desc) = reference.description {
                parts.push(desc.clone());
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n\n"))
        }
    }

    /// Deduplicated tags from primary + references
    pub fn display_merged_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self.tags.clone();

        for reference in &self.references {
            for tag in &reference.tags {
                if !tags.contains(tag) {
                    tags.push(tag.clone());
                }
            }
        }

        tags
    }
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

    pub fn display_tags(&self) -> String {
        self.tags
            .iter()
            .map(|t| format!("+{}", t))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

#[derive(Debug, PartialEq)]
pub enum LinkingWarning {
    OrphanReference {
        id: u32,
        location: Location,
    },
    DuplicatePrimary {
        id: u32,
        duplicate_location: Location,
        first_location: Location,
    },
}

impl fmt::Display for LinkingWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkingWarning::OrphanReference { id, location } => {
                write!(
                    f,
                    "Warning: TODO &{} references non-existent primary #{} at `{}`",
                    id, id, location
                )
            }
            LinkingWarning::DuplicatePrimary {
                id,
                duplicate_location,
                first_location,
            } => {
                write!(
                    f,
                    "Warning: Duplicate TODO #{} found at `{}`, ignoring (first occurrence at `{}`)",
                    id, duplicate_location, first_location
                )
            }
        }
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

    pub fn link_references(self) -> Self {
        let mut warnings = Vec::new();
        let mut primaries: Vec<Todo> = Vec::new();
        let mut primary_index: HashMap<u32, usize> = HashMap::new();
        let mut references: Vec<Todo> = Vec::new();

        // Separate primaries (including todos with no ID) and references
        for todo in self.0 {
            match &todo.id {
                Some(TodoIdentifier::Reference(_)) => {
                    references.push(todo);
                }
                Some(TodoIdentifier::Primary(id)) => {
                    if let Some(&existing_idx) = primary_index.get(id) {
                        // Duplicate primary - warn and ignore
                        let existing = &primaries[existing_idx];
                        warnings.push(LinkingWarning::DuplicatePrimary {
                            id: *id,
                            duplicate_location: todo.location.clone(),
                            first_location: existing.location.clone(),
                        });
                    } else {
                        primary_index.insert(*id, primaries.len());
                        primaries.push(todo);
                    }
                }
                None => {
                    // Todos without IDs are treated as primaries
                    primaries.push(todo);
                }
            }
        }

        // Link references to their primaries
        for reference in references {
            if let Some(TodoIdentifier::Reference(ref_id)) = &reference.id {
                if let Some(&primary_idx) = primary_index.get(ref_id) {
                    primaries[primary_idx].references.push(reference);
                } else {
                    // Orphan reference - warn and discard
                    warnings.push(LinkingWarning::OrphanReference {
                        id: *ref_id,
                        location: reference.location.clone(),
                    });
                }
            }
        }

        for warning in warnings {
            eprintln!("{}", warning);
        }
        Todos(primaries)
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

#[cfg(test)]
mod tests {
    use super::*;
    use parser::TodoInfoBuilder;

    #[test]
    fn test_todos() {
        let todos: Todos = vec![
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(1)))
                    .build()
                    .unwrap(),
                Location::default(),
            ),
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(2)))
                    .build()
                    .unwrap(),
                Location::default(),
            ),
        ]
        .into();
        assert_eq!(todos.get_max_id(), 2);
    }
}
