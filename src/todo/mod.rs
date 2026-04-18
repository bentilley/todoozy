pub mod editor;
pub mod error;
pub mod filter;
pub mod parser;
pub mod sort;
pub mod syntax;

use std::fs::File;
use std::io::{self, prelude::*, BufReader, BufWriter};
use std::path::PathBuf;

use crate::fs::FileTypeAwarePath;

use tempfile::NamedTempFile;

use std::collections::HashMap;

pub use error::Result;
pub use syntax::{TodoInfo, TodoInfoBuilder};

#[derive(Clone, Debug, PartialEq)]
pub enum TodoIdentifier {
    Primary(u32),
    Reference(u32),
}

impl std::ops::Deref for TodoIdentifier {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        match self {
            TodoIdentifier::Primary(id) => id,
            TodoIdentifier::Reference(id) => id,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Metadata(HashMap<String, Vec<String>>);

impl Metadata {
    pub fn new() -> Self {
        Metadata(HashMap::new())
    }

    /// Returns all values for the given key.
    pub fn get(&self, key: &str) -> Option<&[String]> {
        self.0.get(key).map(|v| v.as_slice())
    }

    /// Appends a value for the given key. Multiple values can be set for the same key.
    pub fn set(&mut self, key: &str, value: &str) {
        self.0
            .entry(key.to_string())
            .or_default()
            .push(value.to_string());
    }

    /// Returns an iterator over all (key, value) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0
            .iter()
            .flat_map(|(k, vs)| vs.iter().map(move |v| (k, v)))
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
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
            metadata.set(&key, &value);
        }
        metadata
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Location {
    pub file_path: Option<PathBuf>,
    pub start_line_num: usize,
    pub end_line_num: usize,
}

impl std::fmt::Display for Location {
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
    pub fn new<P: Into<PathBuf>>(file: Option<P>, line_number: usize, end_line_number: usize) -> Self {
        Location {
            file_path: file.map(Into::into),
            start_line_num: line_number,
            end_line_num: end_line_number,
        }
    }

    fn file_location_prefix(&self) -> String {
        self.file_path
            .as_ref()
            .map_or("".to_string(), |p| format!("{}:", p.display()))
    }

    pub fn from_file_line<P: Into<PathBuf>>(file: Option<P>, line_number: usize) -> Self {
        Location {
            file_path: file.map(Into::into),
            start_line_num: line_number,
            end_line_num: line_number,
        }
    }

    pub fn file_path_string(&self) -> Option<String> {
        self.file_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
    }

    pub fn display_start(&self) -> String {
        format!("{}{}", self.file_location_prefix(), self.start_line_num)
    }

    pub fn load(&self, parser: &parser::TodoParser) -> Result<Todo> {
        let file_path = self
            .file_path
            .as_ref()
            .ok_or("Cannot load: No file path in location")?;

        let path = file_path.as_path();
        let file_type = path
            .get_filetype_from_name()
            .ok_or_else(|| format!("Cannot load: Unknown file type for {}", path.display()))?;

        let file = File::open(path)
            .map_err(|e| format!("Cannot load: Failed to open file {}: {}", path.display(), e))?;
        let reader = BufReader::new(file);

        // Extract lines from start to end (1-indexed)
        let lines: Vec<String> = reader
            .lines()
            .enumerate()
            .filter_map(|(i, line)| {
                let line_num = i + 1;
                if line_num >= self.start_line_num && line_num <= self.end_line_num {
                    line.ok()
                } else {
                    None
                }
            })
            .collect();

        let text = lines.join("\n");

        match parser.parse_text(&text, file_type).pop() {
            Some(todo) => Ok(todo),
            None => Err(format!(
                "Cannot load: No TODO found at {}:{}",
                path.display(), self.start_line_num
            )
            .into()),
        }
    }
}

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

    fn try_from(raw_todo: crate::lang::RawTodo) -> std::result::Result<Self, Self::Error> {
        let (start, end, text) = raw_todo;
        let location = Location::new(None::<PathBuf>, start, end);
        let info = syntax::TodoInfo::try_from(text.as_str()).map_err(|e| format!("{}", e))?;
        Ok(Todo::new(info, location))
    }
}

impl Todo {
    pub fn new(info: syntax::TodoInfo, location: Location) -> Self {
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

    pub fn write_id(&self) -> Result<()> {
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
        let file = File::open(file_path)?;
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
        std::fs::copy(tmp_file.path(), file_path)?;

        Ok(())
    }

    pub fn remove(&self) -> Result<()> {
        let file_path = self
            .location
            .file_path
            .as_ref()
            .ok_or("Cannot remove: No file path in location")?;

        let file = File::open(file_path)?;
        let reader = BufReader::new(file);
        let tmp_file = NamedTempFile::new()?;
        let mut writer = BufWriter::new(tmp_file.as_file());

        for (i, line) in reader.lines().enumerate() {
            let line_num = i + 1;
            let content = line?;
            if line_num >= self.location.start_line_num && line_num <= self.location.end_line_num {
                continue;
            }
            writeln!(writer, "{}", content)?;
        }

        writer.flush()?;
        std::fs::copy(tmp_file.path(), file_path)?;
        Ok(())
    }

    pub fn import(&mut self, id: u32) -> Result<()> {
        match &self.id {
            Some(TodoIdentifier::Primary(existing)) => {
                return Err(format!("Todo already has ID #{}", existing).into())
            }
            Some(TodoIdentifier::Reference(ref_id)) => {
                return Err(format!("Cannot import reference todo &{}", ref_id).into())
            }
            None => {}
        }
        self.id = Some(TodoIdentifier::Primary(id));
        self.write_id()
    }

    pub fn editor_command(&self) -> Result<editor::EditorCommand> {
        let file_path = self
            .location
            .file_path
            .as_ref()
            .ok_or("Cannot edit: No file path in location")?;
        Ok(editor::EditorCommand::from_env()?
            .with_location(file_path, self.location.start_line_num))
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

    /// Load TODO content from its source file location.
    ///
    /// This reads the file at `self.location.file_path`, extracts the lines from `start_line_num`
    /// to `end_line_num`, parses that text to get the TODO content, and updates this Todo's fields
    /// (title, priority, tags, etc.).
    ///
    /// Lifecycle data (creation_date, completion_date) is preserved.
    pub fn load(&mut self, parser: &parser::TodoParser) -> Result<()> {
        let loaded = self.location.load(parser)?;
        self.id = loaded.id;
        self.priority = loaded.priority;
        self.title = loaded.title;
        self.description = loaded.description;
        self.tags = loaded.tags;
        self.metadata = loaded.metadata;
        Ok(())
    }
}

impl std::fmt::Display for Todo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let completed_marker = if self.completion_date.is_some() {
            "[x]"
        } else {
            "[ ]"
        };
        let created_date_str = self
            .creation_date
            .map_or("          ".to_string(), |d| format!("{}", d));
        write!(
            f,
            "{} ({}) {} {} {} {}",
            completed_marker,
            created_date_str,
            self.display_id(),
            self.display_priority(),
            self.title,
            self.display_tags()
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
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

impl std::fmt::Display for LinkingWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

#[derive(Clone)]
pub struct Todos {
    imported: HashMap<u32, Todo>,
    unimported: Vec<Todo>,
    warnings: Vec<LinkingWarning>,
}

impl Todos {
    fn new() -> Self {
        Self {
            imported: HashMap::new(),
            unimported: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn warnings(&self) -> &[LinkingWarning] {
        &self.warnings
    }

    pub fn len(&self) -> usize {
        self.imported.len() + self.unimported.len()
    }

    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.imported.keys().copied()
    }

    pub fn get_max_id(&self) -> u32 {
        self.ids().max().unwrap_or(0)
    }

    pub fn has(&self, id: u32) -> bool {
        self.imported.contains_key(&id)
    }

    pub fn get(&self, id: &u32) -> Option<&Todo> {
        self.imported.get(id)
    }

    pub fn insert(&mut self, id: u32, todo: Todo) {
        self.imported.insert(id, todo);
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&u32, &mut Todo)> {
        self.imported.iter_mut()
    }

    /// Returns an iterator over all todos (primaries + unimported, excluding unlinked references)
    pub fn iter(&self) -> impl Iterator<Item = &Todo> {
        self.imported.values().chain(self.unimported.iter())
    }

    // /// Returns a sorted Vec of all todos (primaries + unimported)
    // pub fn sorted<F>(&self, sorter: F) -> Vec<&Todo>
    // where
    //     F: Fn(&Todo, &Todo) -> std::cmp::Ordering,
    // {
    //     let mut todos: Vec<&Todo> = self.iter().collect();
    //     todos.sort_by(|a, b| sorter(a, b));
    //     todos
    // }

    /// Consumes self and returns a sorted Vec of all todos
    pub fn into_sorted<F>(self, sorter: F) -> Vec<Todo>
    where
        F: Fn(&Todo, &Todo) -> std::cmp::Ordering,
    {
        let mut all: Vec<Todo> = self.into();
        all.sort_by(sorter);
        all
    }

    pub fn apply_filter<F>(&mut self, filter: F)
    where
        F: Fn(&Todo) -> bool,
    {
        self.imported.retain(|_, todo| filter(todo));
        self.unimported.retain(filter);
    }

    /// Merge another Todos into this one.
    ///
    /// Imported todos from `other` override existing ones with the same ID.
    /// Unimported todos from `other` are appended.
    /// Warnings from `other` are appended.
    pub fn merge(&mut self, other: Todos) {
        for (id, todo) in other.imported {
            self.imported.insert(id, todo);
        }
        self.unimported.extend(other.unimported);
        self.warnings.extend(other.warnings);
    }
}

impl From<Vec<Todo>> for Todos {
    fn from(todos: Vec<Todo>) -> Self {
        let mut result = Todos::new();
        let mut references: HashMap<u32, Vec<Todo>> = HashMap::new();

        for todo in todos {
            match &todo.id {
                Some(TodoIdentifier::Primary(id)) => {
                    if let Some(existing) = result.imported.get(id) {
                        result.warnings.push(LinkingWarning::DuplicatePrimary {
                            id: *id,
                            duplicate_location: todo.location.clone(),
                            first_location: existing.location.clone(),
                        });
                    } else {
                        result.imported.insert(*id, todo);
                    }
                }
                Some(TodoIdentifier::Reference(id)) => {
                    references.entry(*id).or_default().push(todo);
                }
                None => {
                    result.unimported.push(todo);
                }
            }
        }

        // Move references into their corresponding primaries
        for (ref_id, refs) in std::mem::take(&mut references) {
            if let Some(primary) = result.imported.get_mut(&ref_id) {
                primary.references.extend(refs);
            } else {
                // Orphan references - warn and discard
                for reference in refs {
                    result.warnings.push(LinkingWarning::OrphanReference {
                        id: ref_id,
                        location: reference.location.clone(),
                    });
                }
            }
        }

        result
    }
}

impl From<Todos> for Vec<Todo> {
    fn from(todos: Todos) -> Self {
        let mut result: Vec<Todo> = todos
            .imported
            .into_values()
            .map(|t| {
                let mut all = t.references.to_vec();
                all.push(t);
                all
            })
            .flatten()
            .collect();
        result.extend(todos.unimported);
        result
    }
}

impl From<HashMap<u32, Todo>> for Todos {
    fn from(map: HashMap<u32, Todo>) -> Self {
        Todos {
            imported: map,
            unimported: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

impl From<Todos> for HashMap<u32, Todo> {
    fn from(todos: Todos) -> Self {
        todos.imported
    }
}

impl IntoIterator for Todos {
    type Item = Todo;
    type IntoIter = std::vec::IntoIter<Todo>;

    fn into_iter(self) -> Self::IntoIter {
        let all: Vec<Todo> = self.into();
        all.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntax::TodoInfoBuilder;

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

    #[test]
    fn test_metadata_get_missing_key() {
        let metadata = Metadata::new();
        assert_eq!(metadata.get("nonexistent"), None);
    }

    #[test]
    fn test_metadata_single_value() {
        let mut metadata = Metadata::new();
        metadata.set("key", "value");

        assert_eq!(
            metadata.get("key"),
            Some(vec!["value".to_string()].as_slice())
        );
        assert!(metadata.contains_key("key"));
        assert!(!metadata.is_empty());
        assert_eq!(metadata.len(), 1);
    }

    #[test]
    fn test_metadata_multi_value() {
        let mut metadata = Metadata::new();
        metadata.set("depends", "42");
        metadata.set("depends", "41");
        metadata.set("depends", "40");

        assert_eq!(metadata.len(), 1);
        assert_eq!(
            metadata.keys().collect::<Vec<_>>(),
            vec![&"depends".to_string()]
        );

        assert_eq!(
            metadata.get("depends"), // get() returns values in insertion order
            Some(vec!["42".to_string(), "41".to_string(), "40".to_string()].as_slice())
        );
    }

    #[test]
    fn test_metadata_iter_flattens() {
        let mut metadata = Metadata::new();
        metadata.set("depends", "42");
        metadata.set("depends", "41");
        metadata.set("owner", "alice");

        assert_eq!(metadata.len(), 2);
        let keys = metadata.keys().collect::<Vec<_>>();
        assert_eq!(keys.contains(&&"depends".to_string()), true);
        assert_eq!(keys.contains(&&"owner".to_string()), true);

        let pairs: Vec<_> = metadata.iter().collect();
        assert_eq!(pairs.len(), 3);
    }

    #[test]
    fn test_metadata_from_iterator() {
        let metadata: Metadata = vec![
            ("key1".to_string(), "value1".to_string()),
            ("key1".to_string(), "value2".to_string()),
            ("key2".to_string(), "value3".to_string()),
        ]
        .into_iter()
        .collect();

        assert_eq!(metadata.len(), 2);
        let keys = metadata.keys().collect::<Vec<_>>();
        assert_eq!(keys.contains(&&"key1".to_string()), true);
        assert_eq!(keys.contains(&&"key2".to_string()), true);

        assert_eq!(
            metadata.get("key1"),
            Some(vec!["value1".to_string(), "value2".to_string()].as_slice())
        );
        assert_eq!(metadata.get("key2").unwrap()[0], "value3");
    }

    #[test]
    fn test_todos_iter() {
        let todos: Todos = vec![
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(1)))
                    .title("Primary 1".to_string())
                    .build()
                    .unwrap(),
                Location::default(),
            ),
            Todo::new(
                TodoInfoBuilder::default()
                    .id(None)
                    .title("Unimported".to_string())
                    .build()
                    .unwrap(),
                Location::default(),
            ),
        ]
        .into();

        let titles: Vec<_> = todos.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles.len(), 2);
        assert!(titles.contains(&"Primary 1"));
        assert!(titles.contains(&"Unimported"));
    }

    #[test]
    fn test_todos_into_sorted() {
        let todos: Todos = vec![
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(3)))
                    .title("C".to_string())
                    .build()
                    .unwrap(),
                Location::default(),
            ),
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(1)))
                    .title("A".to_string())
                    .build()
                    .unwrap(),
                Location::default(),
            ),
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(2)))
                    .title("B".to_string())
                    .build()
                    .unwrap(),
                Location::default(),
            ),
        ]
        .into();

        let sorted = todos.into_sorted(|a, b| a.title.cmp(&b.title));
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].title, "A");
        assert_eq!(sorted[1].title, "B");
        assert_eq!(sorted[2].title, "C");
    }

    #[test]
    fn test_todos_apply_filter() {
        let mut todos: Todos = vec![
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(1)))
                    .priority(Some('A'))
                    .build()
                    .unwrap(),
                Location::default(),
            ),
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(2)))
                    .priority(Some('C'))
                    .build()
                    .unwrap(),
                Location::default(),
            ),
            Todo::new(
                TodoInfoBuilder::default()
                    .id(None)
                    .priority(Some('A'))
                    .build()
                    .unwrap(),
                Location::default(),
            ),
        ]
        .into();

        todos.apply_filter(|t| t.priority == Some('A'));
        let remaining: Vec<_> = todos.iter().collect();
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().all(|t| t.priority == Some('A')));
    }

    #[test]
    fn test_todos_reference_linking() {
        let todos: Todos = vec![
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(1)))
                    .title("Primary".to_string())
                    .build()
                    .unwrap(),
                Location::default(),
            ),
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Reference(1)))
                    .title("Reference to 1".to_string())
                    .build()
                    .unwrap(),
                Location::default(),
            ),
        ]
        .into();

        // Reference should be linked to primary, not in iter directly
        let primaries: Vec<_> = todos.iter().collect();
        assert_eq!(primaries.len(), 1);
        assert_eq!(primaries[0].title, "Primary");
        assert_eq!(primaries[0].references.len(), 1);
        assert_eq!(primaries[0].references[0].title, "Reference to 1");
    }

    #[test]
    fn test_todos_orphan_reference_warning() {
        let todos: Todos = vec![Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Reference(999)))
                .title("Orphan reference".to_string())
                .build()
                .unwrap(),
            Location::new(Some("test.rs".to_string()), 10, 10),
        )]
        .into();

        assert_eq!(todos.warnings.len(), 1);
        match &todos.warnings[0] {
            LinkingWarning::OrphanReference { id, location } => {
                assert_eq!(*id, 999);
                assert_eq!(location.start_line_num, 10);
            }
            _ => panic!("Expected OrphanReference warning"),
        }
    }

    #[test]
    fn test_todos_duplicate_primary_warning() {
        let todos: Todos = vec![
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(1)))
                    .title("First".to_string())
                    .build()
                    .unwrap(),
                Location::new(Some("a.rs".to_string()), 5, 5),
            ),
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(1)))
                    .title("Duplicate".to_string())
                    .build()
                    .unwrap(),
                Location::new(Some("b.rs".to_string()), 10, 10),
            ),
        ]
        .into();

        assert_eq!(todos.warnings.len(), 1);
        match &todos.warnings[0] {
            LinkingWarning::DuplicatePrimary {
                id,
                duplicate_location,
                first_location,
            } => {
                assert_eq!(*id, 1);
                assert_eq!(first_location.start_line_num, 5);
                assert_eq!(duplicate_location.start_line_num, 10);
            }
            _ => panic!("Expected DuplicatePrimary warning"),
        }

        // Only the first occurrence should be kept
        assert_eq!(todos.iter().count(), 1);
        assert_eq!(todos.iter().next().unwrap().title, "First");
    }

    #[test]
    fn test_todos_from_hashmap() {
        let mut map: HashMap<u32, Todo> = HashMap::new();
        map.insert(
            1,
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(1)))
                    .build()
                    .unwrap(),
                Location::default(),
            ),
        );
        map.insert(
            2,
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(2)))
                    .build()
                    .unwrap(),
                Location::default(),
            ),
        );

        let todos: Todos = map.into();
        assert_eq!(todos.get_max_id(), 2);
        assert_eq!(todos.iter().count(), 2);
    }

    #[test]
    fn test_todos_into_hashmap() {
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
            Todo::new(
                TodoInfoBuilder::default().id(None).build().unwrap(),
                Location::default(),
            ),
        ]
        .into();

        let map: HashMap<u32, Todo> = todos.into();
        // HashMap conversion only includes primaries, not unimported
        assert_eq!(map.len(), 2);
        assert!(map.contains_key(&1));
        assert!(map.contains_key(&2));
    }

    #[test]
    fn test_todos_into_vec() {
        let todos: Todos = vec![
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(1)))
                    .title("Primary".to_string())
                    .build()
                    .unwrap(),
                Location::default(),
            ),
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Reference(1)))
                    .title("Ref".to_string())
                    .build()
                    .unwrap(),
                Location::default(),
            ),
            Todo::new(
                TodoInfoBuilder::default()
                    .id(None)
                    .title("Unimported".to_string())
                    .build()
                    .unwrap(),
                Location::default(),
            ),
        ]
        .into();

        let vec: Vec<Todo> = todos.into();
        // Vec includes primary, its references, and unimported
        assert_eq!(vec.len(), 3);
        let titles: Vec<_> = vec.iter().map(|t| t.title.as_str()).collect();
        assert!(titles.contains(&"Primary"));
        assert!(titles.contains(&"Ref"));
        assert!(titles.contains(&"Unimported"));
    }

    #[test]
    fn test_todos_into_iterator() {
        let todos: Todos = vec![
            Todo::new(
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Primary(1)))
                    .build()
                    .unwrap(),
                Location::default(),
            ),
            Todo::new(
                TodoInfoBuilder::default().id(None).build().unwrap(),
                Location::default(),
            ),
        ]
        .into();

        let collected: Vec<Todo> = todos.into_iter().collect();
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn test_todos_empty() {
        let todos: Todos = Vec::new().into();
        assert_eq!(todos.get_max_id(), 0);
        assert_eq!(todos.iter().count(), 0);
        assert!(todos.warnings.is_empty());
    }

    #[test]
    fn test_todos_merge_imported_override() {
        let mut base: Todos = vec![Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Primary(1)))
                .title("Original".to_string())
                .build()
                .unwrap(),
            Location::default(),
        )]
        .into();

        let other: Todos = vec![Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Primary(1)))
                .title("Updated".to_string())
                .build()
                .unwrap(),
            Location::default(),
        )]
        .into();

        base.merge(other);

        assert_eq!(base.iter().count(), 1);
        assert_eq!(base.get(&1).unwrap().title, "Updated");
    }

    #[test]
    fn test_todos_merge_adds_new_imported() {
        let mut base: Todos = vec![Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Primary(1)))
                .title("First".to_string())
                .build()
                .unwrap(),
            Location::default(),
        )]
        .into();

        let other: Todos = vec![Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Primary(2)))
                .title("Second".to_string())
                .build()
                .unwrap(),
            Location::default(),
        )]
        .into();

        base.merge(other);

        assert_eq!(base.iter().count(), 2);
        assert!(base.has(1));
        assert!(base.has(2));
    }

    #[test]
    fn test_todos_merge_unimported() {
        let mut base: Todos = vec![Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Primary(1)))
                .title("Imported".to_string())
                .build()
                .unwrap(),
            Location::default(),
        )]
        .into();

        let other: Todos = vec![Todo::new(
            TodoInfoBuilder::default()
                .id(None)
                .title("Unimported".to_string())
                .build()
                .unwrap(),
            Location::default(),
        )]
        .into();

        base.merge(other);

        assert_eq!(base.iter().count(), 2);
        let titles: Vec<_> = base.iter().map(|t| t.title.as_str()).collect();
        assert!(titles.contains(&"Imported"));
        assert!(titles.contains(&"Unimported"));
    }

    #[test]
    fn test_todos_merge_warnings() {
        let mut base: Todos = Todos::new();

        let other: Todos = vec![Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Reference(999)))
                .title("Orphan".to_string())
                .build()
                .unwrap(),
            Location::new(Some("test.rs".to_string()), 1, 1),
        )]
        .into();

        assert_eq!(other.warnings().len(), 1);

        base.merge(other);

        assert_eq!(base.warnings().len(), 1);
        matches!(&base.warnings()[0], LinkingWarning::OrphanReference { id: 999, .. });
    }
}
