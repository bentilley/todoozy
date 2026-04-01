use core::fmt::{self, Display};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

mod parser;

pub trait Filter: Display + std::fmt::Debug {
    fn filter(&self, todo: &crate::todo::Todo) -> bool;
    fn box_clone(&self) -> Box<dyn Filter>;
}

impl Clone for Box<dyn Filter> {
    fn clone(&self) -> Box<dyn Filter> {
        self.box_clone()
    }
}

impl Serialize for Box<dyn Filter> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Box<dyn Filter> {
    fn deserialize<D>(deserializer: D) -> Result<Box<dyn Filter>, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_str(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, PartialEq, Clone)]
enum Property {
    File,
    Priority,
    Tag,
    CreationDate,
    CompletionDate,
}

impl Display for Property {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Property::File => write!(f, "file"),
            Property::Priority => write!(f, "priority"),
            Property::Tag => write!(f, "tag"),
            Property::CreationDate => write!(f, "creation_date"),
            Property::CompletionDate => write!(f, "completion_date"),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
enum Relation {
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
}

impl Display for Relation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Relation::Equal => write!(f, "="),
            Relation::NotEqual => write!(f, "!="),
            Relation::Greater => write!(f, ">"),
            Relation::GreaterEqual => write!(f, ">="),
            Relation::Less => write!(f, "<"),
            Relation::LessEqual => write!(f, "<="),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct PropertyFilter {
    property: Property,
    relation: Relation,
    pub value: String,
}

impl Filter for PropertyFilter {
    fn filter(&self, todo: &crate::todo::Todo) -> bool {
        match self.property {
            Property::File => match todo.location.file_path.clone() {
                Some(file_path) => match self.relation {
                    Relation::Equal => file_path == self.value,
                    Relation::NotEqual => file_path != self.value,
                    Relation::Greater => file_path > self.value,
                    Relation::GreaterEqual => file_path >= self.value,
                    Relation::Less => file_path < self.value,
                    Relation::LessEqual => file_path <= self.value,
                },
                None => false,
            },
            Property::Priority => {
                let priority = todo.priority.unwrap_or('Z');
                let value = self.value.chars().next().unwrap();
                match self.relation {
                    Relation::Equal => priority == value,
                    Relation::NotEqual => priority != value,
                    // These are reversed because char 'A' is actually < char 'B' etc. I guess it's
                    // done based on their ASCII values...
                    Relation::Greater => priority < value,
                    Relation::GreaterEqual => priority <= value,
                    Relation::Less => priority > value,
                    Relation::LessEqual => priority >= value,
                }
            }
            Property::Tag => match self.relation {
                Relation::Equal => todo.has_tag(&self.value),
                Relation::NotEqual => !todo.has_tag(&self.value),
                _ => false,
            },
            Property::CreationDate => {
                let date = match self.value.parse::<chrono::NaiveDate>() {
                    Ok(date) => date,
                    Err(e) => {
                        eprintln!("Error parsing date: {}", e);
                        return false;
                    }
                };
                match self.relation {
                    Relation::Equal => todo.creation_date == Some(date),
                    Relation::NotEqual => todo.creation_date != Some(date),
                    Relation::Greater => todo.creation_date > Some(date),
                    Relation::GreaterEqual => todo.creation_date >= Some(date),
                    Relation::Less => todo.creation_date < Some(date),
                    Relation::LessEqual => todo.creation_date <= Some(date),
                }
            }
            Property::CompletionDate => {
                let date = self.value.parse::<chrono::NaiveDate>().unwrap();
                match self.relation {
                    Relation::Equal => todo.completion_date == Some(date),
                    Relation::NotEqual => todo.completion_date != Some(date),
                    Relation::Greater => todo.completion_date > Some(date),
                    Relation::GreaterEqual => todo.completion_date >= Some(date),
                    Relation::Less => todo.completion_date < Some(date),
                    Relation::LessEqual => todo.completion_date <= Some(date),
                }
            }
        }
    }

    fn box_clone(&self) -> Box<dyn Filter> {
        Box::new(self.clone())
    }
}

// impl Serialize for PropertyFilter {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: serde::ser::Serializer,
//     {
//         format!("{}", self).serialize(serializer)
//     }
// }

impl Display for PropertyFilter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}{}", self.property, self.relation, self.value)
    }
}

#[derive(Debug, Clone)]
pub struct Disjunction {
    pub filters: Vec<Box<dyn Filter>>,
}

impl Filter for Disjunction {
    fn filter(&self, todo: &crate::todo::Todo) -> bool {
        self.filters.iter().any(|clause| clause.filter(todo))
    }

    fn box_clone(&self) -> Box<dyn Filter> {
        Box::new(self.clone())
    }
}

impl Display for Disjunction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let filters: Vec<String> = self.filters.iter().map(|f| format!("{}", f)).collect();
        if filters.len() == 1 {
            write!(f, "{}", filters[0])
        } else {
            write!(f, "({})", filters.join(" or "))
        }
    }
}

#[derive(Debug, Clone)]
pub struct Conjunction {
    pub filters: Vec<Box<dyn Filter>>,
}

impl Filter for Conjunction {
    fn filter(&self, todo: &crate::todo::Todo) -> bool {
        self.filters.iter().all(|clause| clause.filter(todo))
    }

    fn box_clone(&self) -> Box<dyn Filter> {
        Box::new(self.clone())
    }
}

impl Display for Conjunction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let filters: Vec<String> = self.filters.iter().map(|f| format!("{}", f)).collect();
        if filters.len() == 1 {
            write!(f, "{}", filters[0])
        } else {
            write!(f, "({})", filters.join(" and "))
        }
    }
}

#[derive(Debug, Clone)]
pub struct Negation {
    pub filter: Box<dyn Filter>,
}

impl Filter for Negation {
    fn filter(&self, todo: &crate::todo::Todo) -> bool {
        !self.filter.filter(todo)
    }

    fn box_clone(&self) -> Box<dyn Filter> {
        Box::new(self.clone())
    }
}

impl Display for Negation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "not {}", self.filter)
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct All {}

impl Filter for All {
    fn filter(&self, _todo: &crate::todo::Todo) -> bool {
        true
    }

    fn box_clone(&self) -> Box<dyn Filter> {
        Box::new(self.clone())
    }
}

impl Display for All {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "all")
    }
}

pub fn parse_str(filter_def: &str) -> Result<Box<dyn Filter>, String> {
    self::parser::parse_expression(&filter_def)
}

impl FromStr for Box<dyn Filter> {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo::{TodoInfoBuilder, Location, Todo};

    #[test]
    fn test_serialize_json_filter() {
        let filter: Box<dyn Filter> = Box::new(PropertyFilter {
            property: Property::Priority,
            relation: Relation::Equal,
            value: "A".to_string(),
        });
        assert_eq!(serde_json::to_string(&filter).unwrap(), "\"priority=A\"");
    }

    #[test]
    fn test_deserialize_json_filter() {
        let filter: Box<dyn Filter> = serde_json::from_str("\"priority=A\"").unwrap();
        let todo_true = Todo::new(
            TodoInfoBuilder::default()
                .priority(Some('A'))
                .build()
                .unwrap(),
            Location::default(),
        );
        let todo_false = Todo::new(
            TodoInfoBuilder::default()
                .priority(Some('B'))
                .build()
                .unwrap(),
            Location::default(),
        );
        assert_eq!(filter.filter(&todo_true), true);
        assert_eq!(filter.filter(&todo_false), false);
    }

    #[test]
    fn test_display_property_filter() {
        let filter = PropertyFilter {
            property: Property::Priority,
            relation: Relation::GreaterEqual,
            value: "A".to_string(),
        };
        assert_eq!(format!("{}", filter), "priority>=A");
    }

    #[test]
    fn test_property_filter() {
        let filter = PropertyFilter {
            property: Property::Priority,
            relation: Relation::Equal,
            value: "A".to_string(),
        };
        assert_eq!(
            filter.filter(&Todo::new(
                TodoInfoBuilder::default()
                    .priority(Some('A'))
                    .build()
                    .unwrap(),
                Location::default(),
            )),
            true
        );

        let filter = PropertyFilter {
            property: Property::Priority,
            relation: Relation::Greater,
            value: "A".to_string(),
        };
        assert_eq!(
            filter.filter(&Todo::new(
                TodoInfoBuilder::default()
                    .priority(Some('B'))
                    .build()
                    .unwrap(),
                Location::default(),
            )),
            false
        );
    }

    #[test]
    fn test_display_disjunction() {
        let filter = Disjunction {
            filters: vec![
                Box::new(PropertyFilter {
                    property: Property::Priority,
                    relation: Relation::Equal,
                    value: "A".to_string(),
                }),
                Box::new(PropertyFilter {
                    property: Property::Priority,
                    relation: Relation::NotEqual,
                    value: "B".to_string(),
                }),
            ],
        };
        assert_eq!(format!("{}", filter), "(priority=A or priority!=B)");
    }

    #[test]
    fn test_display_conjunction() {
        let filter = Conjunction {
            filters: vec![
                Box::new(PropertyFilter {
                    property: Property::Priority,
                    relation: Relation::Greater,
                    value: "A".to_string(),
                }),
                Box::new(PropertyFilter {
                    property: Property::Priority,
                    relation: Relation::LessEqual,
                    value: "B".to_string(),
                }),
            ],
        };
        assert_eq!(format!("{}", filter), "(priority>A and priority<=B)");
    }

    #[test]
    fn test_display_negation() {
        let filter = Negation {
            filter: Box::new(PropertyFilter {
                property: Property::Priority,
                relation: Relation::Equal,
                value: "A".to_string(),
            }),
        };
        assert_eq!(format!("{}", filter), "not priority=A");
    }
}
