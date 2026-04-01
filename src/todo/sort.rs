use core::fmt::Display;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

mod parser;

pub trait Sorter: Display + std::fmt::Debug {
    fn compare(&self, a: &crate::todo::Todo, b: &crate::todo::Todo) -> std::cmp::Ordering;
    fn box_clone(&self) -> Box<dyn Sorter>;
}

impl Clone for Box<dyn Sorter> {
    fn clone(&self) -> Box<dyn Sorter> {
        self.box_clone()
    }
}

impl Serialize for Box<dyn Sorter> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Box<dyn Sorter> {
    fn deserialize<D>(deserializer: D) -> Result<Box<dyn Sorter>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_str(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, PartialEq, Clone)]
enum Property {
    Title,
    File,
    LineNumber,
    Priority,
    CreationDate,
    CompletionDate,
    // TODO #9 (Z) 2024-08-31 Can you sort by project and context?
    // Project,
    // Context,
}

impl Display for Property {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Property::Title => write!(f, "title"),
            Property::File => write!(f, "file"),
            Property::LineNumber => write!(f, "line_number"),
            Property::Priority => write!(f, "priority"),
            Property::CreationDate => write!(f, "creation_date"),
            Property::CompletionDate => write!(f, "completion_date"),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
enum Direction {
    Ascending,
    Descending,
}

impl Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Direction::Ascending => write!(f, "asc"),
            Direction::Descending => write!(f, "desc"),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
struct PropertySorter {
    property: Property,
    direction: Direction,
}

impl Default for PropertySorter {
    fn default() -> Self {
        PropertySorter {
            property: Property::Priority,
            direction: Direction::Ascending,
        }
    }
}

impl Sorter for PropertySorter {
    fn compare(&self, a: &crate::todo::Todo, b: &crate::todo::Todo) -> std::cmp::Ordering {
        let ord = match self.property {
            Property::Title => a.title.cmp(&b.title),
            Property::File => a.location.file_path.cmp(&b.location.file_path),
            Property::LineNumber => a.location.start_line_num.cmp(&b.location.start_line_num),
            Property::Priority => {
                let a = a.priority.unwrap_or('Z');
                let b = b.priority.unwrap_or('Z');
                a.cmp(&b)
            }
            Property::CreationDate => a.creation_date.cmp(&b.creation_date),
            Property::CompletionDate => a.completion_date.cmp(&b.completion_date),
        };
        match self.direction {
            Direction::Ascending => ord,
            Direction::Descending => ord.reverse(),
        }
    }

    fn box_clone(&self) -> Box<dyn Sorter> {
        Box::new(self.clone())
    }
}

impl Display for PropertySorter {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}:{}", self.property, self.direction)
    }
}

#[derive(Default, Debug, Clone)]
pub struct SortPipeline {
    sorters: Vec<Box<dyn Sorter>>,
}

impl SortPipeline {
    fn new(sorters: Vec<Box<dyn Sorter>>) -> Self {
        SortPipeline { sorters }
    }

    pub fn app_default() -> Self {
        SortPipeline {
            sorters: vec![
                Box::new(PropertySorter {
                    property: Property::Priority,
                    direction: Direction::Ascending,
                }),
                Box::new(PropertySorter {
                    property: Property::CreationDate,
                    direction: Direction::Descending,
                }),
            ],
        }
    }

    fn add_sorter(&mut self, sorter: Box<dyn Sorter>) {
        self.sorters.push(sorter);
    }
}

impl Sorter for SortPipeline {
    fn compare(&self, a: &crate::todo::Todo, b: &crate::todo::Todo) -> std::cmp::Ordering {
        for sorter in &self.sorters {
            let ord = sorter.compare(a, b);
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
        std::cmp::Ordering::Equal
    }

    fn box_clone(&self) -> Box<dyn Sorter> {
        Box::new(self.clone())
    }
}

impl Display for SortPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let sorters: Vec<String> = self.sorters.iter().map(|f| format!("{}", f)).collect();
        write!(f, "{}", sorters.join(" > "))
    }
}

pub fn parse_str(sort_def: &str) -> Result<Box<dyn Sorter>, String> {
    self::parser::parse_expression(sort_def)
}

impl FromStr for Box<dyn Sorter> {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo::{Location, Todo, TodoInfoBuilder};

    #[test]
    fn test_serialize_json_sorter() {
        let sorter: Box<dyn Sorter> = Box::new(PropertySorter {
            property: Property::Priority,
            direction: Direction::Ascending,
        });
        let json = serde_json::to_string(&sorter).unwrap();
        assert_eq!(json, "\"priority:asc\"");
    }

    #[test]
    fn test_deserialize_json_sorter() {
        let sorter: Box<dyn Sorter> = serde_json::from_str("\"priority:desc\"").unwrap();
        let mut todos = vec![
            Todo::new(
                TodoInfoBuilder::default()
                    .title("A".to_string())
                    .priority(Some('A'))
                    .build()
                    .unwrap(),
                Location::default(),
            ),
            Todo::new(
                TodoInfoBuilder::default()
                    .title("B".to_string())
                    .priority(Some('B'))
                    .build()
                    .unwrap(),
                Location::default(),
            ),
        ];
        todos.sort_by(|a, b| sorter.compare(a, b));
        assert_eq!(todos[0].title, "B");
        assert_eq!(todos[1].title, "A");
    }
}
