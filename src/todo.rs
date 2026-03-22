pub mod filter;
pub mod parser;
pub mod sort;

// TODO #60 (D) 2026-03-22 Add TodoRef struct for multi-location todos +model +refs
//
// Allow multiple TODO comments to reference the same todo ID. A primary todo
// owns the ID (`#43`), references point to it (`&43`).
//
// New struct:
//   struct TodoRef {
//       id: u32,                    // ID being referenced
//       title: Option<String>,
//       description: Option<String>,
//       projects: Vec<String>,
//       contexts: Vec<String>,
//       metadata: Metadata,
//       file: String,
//       line_number: u32,
//   }
//
// Add to Todo struct:
//   references: Vec<TodoRef>
//
// For display, references roll up into the primary:
// - Reference title becomes a `## Subtitle` in description
// - Reference description appended after subtitle
// - Projects/contexts/metadata merged for display (kept separate in model)
// - Locations list shows all, with `*` marking the primary

use std::fs::File;
use std::io::{self, prelude::*, BufReader, BufWriter};

use derive_builder::Builder;
use tempfile::NamedTempFile;

use std::collections::HashMap;
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
    pub id: Option<u32>,

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
    pub fn location_start(&self) -> String {
        match self.file {
            Some(ref file) => {
                if let Some(line_number) = self.line_number {
                    format!("{}:{}", file, line_number)
                } else {
                    file.clone()
                }
            }
            None => "".to_string(),
        }
    }

    pub fn has_project(&self, project: &str) -> bool {
        self.projects.iter().any(|p| p == project)
    }

    pub fn has_context(&self, context: &str) -> bool {
        self.contexts.iter().any(|c| c == context)
    }

    pub fn write_id(&self) -> Result<(), Box<dyn std::error::Error>> {
        let id = match self.id {
            Some(id) => id,
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

pub struct Todos(pub Vec<Todo>);

impl Todos {
    pub fn get_max_id(&self) -> u32 {
        self.0.iter().map(|t| t.id.unwrap_or(0)).max().unwrap_or(0)
    }
    // pub fn filter(&self, filter: &dyn filter::Filter) -> Vec<Todo> {
    //     self.iter().filter(|t| filter.matches(t)).cloned().collect()
    // }
    // pub fn sort(&self, sorter: &dyn sort::Sorter) -> Vec<Todo> {
    //     let mut todos = self.to_vec();
    //     todos.sort_by(|a, b| sorter.compare(a, b));
    //     todos
    // }
}

#[test]
fn test_todos() {
    let todos = Todos(vec![
        TodoBuilder::default().id(Some(1)).build().unwrap(),
        TodoBuilder::default().id(Some(2)).build().unwrap(),
    ]);
    assert_eq!(todos.get_max_id(), 2);
}

impl IntoIterator for Todos {
    type Item = Todo;
    type IntoIter = std::vec::IntoIter<Todo>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
