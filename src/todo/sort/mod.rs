use std::fmt::Display;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

mod error;
mod parser;
pub use error::{Error, Result};

pub trait Sorter: Display + std::fmt::Debug + SorterClone {
    fn compare(&self, a: &crate::todo::Todo, b: &crate::todo::Todo) -> std::cmp::Ordering;
}

// SorterClone enables cloning of Box<dyn Sorter>. We can't use Clone directly as a
// supertrait because Clone::clone returns Self, which requires Sized - but trait
// objects are unsized. This workaround uses a separate trait with a blanket impl
// that returns Box<dyn Sorter> instead, which has a known size.
pub trait SorterClone {
    fn box_clone(&self) -> Box<dyn Sorter>;
}

impl<T: Sorter + Clone + 'static> SorterClone for T {
    fn box_clone(&self) -> Box<dyn Sorter> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Sorter> {
    fn clone(&self) -> Box<dyn Sorter> {
        self.box_clone()
    }
}

impl Serialize for Box<dyn Sorter> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Box<dyn Sorter> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Box<dyn Sorter>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse::<Box<dyn Sorter>>()
            .map_err(serde::de::Error::custom)
    }
}

// TODO #95 (C) 2026-04-18 Add sort by ID
#[derive(Debug, PartialEq, Clone)]
enum Property {
    Title,
    File,
    LineNumber,
    Priority,
    CreationDate,
    CompletionDate,
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

impl Direction {
    fn apply(&self, ord: std::cmp::Ordering) -> std::cmp::Ordering {
        match self {
            Direction::Ascending => ord,
            Direction::Descending => ord.reverse(),
        }
    }
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
        self.direction.apply(ord)
    }
}

impl Display for PropertySorter {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}:{}", self.property, self.direction)
    }
}

#[derive(Debug, PartialEq, Clone)]
struct TagSorter {
    tag_name: String,
    direction: Direction,
}

impl Sorter for TagSorter {
    fn compare(&self, a: &crate::todo::Todo, b: &crate::todo::Todo) -> std::cmp::Ordering {
        let a_has = a.has_tag(&self.tag_name);
        let b_has = b.has_tag(&self.tag_name);
        let ord = match (a_has, b_has) {
            (true, true) | (false, false) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Less, // has tag comes first
            (false, true) => std::cmp::Ordering::Greater,
        };
        self.direction.apply(ord)
    }
}

impl Display for TagSorter {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "tag:{}:{}", self.tag_name, self.direction)
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

}

impl Sorter for SortPipeline {
    fn compare(&self, a: &crate::todo::Todo, b: &crate::todo::Todo) -> std::cmp::Ordering {
        self.sorters
            .iter()
            .map(|s| s.compare(a, b))
            .find(|&o| o != std::cmp::Ordering::Equal)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl Display for SortPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let sorters: Vec<String> = self.sorters.iter().map(|s| s.to_string()).collect();
        write!(f, "{}", sorters.join(" > "))
    }
}

impl FromStr for Box<dyn Sorter> {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        parser::parse_expression(s)
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
    fn test_serialize_json_sorter_tag() {
        let sorter: Box<dyn Sorter> = Box::new(TagSorter {
            tag_name: "feature".to_string(),
            direction: Direction::Descending,
        });
        let json = serde_json::to_string(&sorter).unwrap();
        assert_eq!(json, "\"tag:feature:desc\"");
    }

    #[test]
    fn test_deserialize_json_sorter_invalid_returns_error() {
        let result: serde_json::Result<Box<dyn Sorter>> =
            serde_json::from_str("\"not_a_valid_sorter\"");
        assert!(result.is_err(), "expected error for invalid sorter string");
    }

    #[test]
    fn test_deserialize_json_sorter_wrong_type_returns_error() {
        let result: serde_json::Result<Box<dyn Sorter>> = serde_json::from_str("42");
        assert!(result.is_err(), "expected error when input is not a string");
    }

    #[test]
    fn test_deserialize_json_sorter_empty_string_returns_error() {
        let result: serde_json::Result<Box<dyn Sorter>> = serde_json::from_str("\"\"");
        assert!(result.is_err(), "expected error for empty sorter string");
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
