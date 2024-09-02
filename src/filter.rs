mod parser;

pub trait Filter {
    fn filter(&self, todo: &crate::todo::Todo) -> bool;
}

#[derive(Debug, PartialEq)]
enum Property {
    File,
    Priority,
    Project,
    Context,
    CreationDate,
    CompletionDate,
}

#[derive(Debug, PartialEq)]
enum Relation {
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
}

#[derive(Debug, PartialEq)]
pub struct PropertyFilter {
    property: Property,
    relation: Relation,
    pub value: String,
}

impl Filter for PropertyFilter {
    fn filter(&self, todo: &crate::todo::Todo) -> bool {
        match self.property {
            Property::File => {
                let value = Some(self.value.clone());
                match self.relation {
                    Relation::Equal => todo.file == value,
                    Relation::NotEqual => todo.file != value,
                    Relation::Greater => todo.file > value,
                    Relation::GreaterEqual => todo.file >= value,
                    Relation::Less => todo.file < value,
                    Relation::LessEqual => todo.file <= value,
                }
            }
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
            Property::Project => match self.relation {
                Relation::Equal => todo.has_project(&self.value),
                Relation::NotEqual => !todo.has_project(&self.value),
                _ => false,
            },
            Property::Context => match self.relation {
                Relation::Equal => todo.has_context(&self.value),
                Relation::NotEqual => !todo.has_context(&self.value),
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
}

#[test]
fn test_property_filter() {
    let filter = PropertyFilter {
        property: Property::Priority,
        relation: Relation::Equal,
        value: "A".to_string(),
    };
    assert_eq!(
        filter.filter(
            &crate::todo::TodoBuilder::default()
                .priority(Some('A'))
                .build()
                .unwrap()
        ),
        true
    );

    let filter = PropertyFilter {
        property: Property::Priority,
        relation: Relation::Greater,
        value: "A".to_string(),
    };
    assert_eq!(
        filter.filter(
            &crate::todo::TodoBuilder::default()
                .priority(Some('B'))
                .build()
                .unwrap()
        ),
        false
    );
}

pub struct Disjunction {
    pub filters: Vec<Box<dyn Filter>>,
}

impl Filter for Disjunction {
    fn filter(&self, todo: &crate::todo::Todo) -> bool {
        self.filters.iter().any(|clause| clause.filter(todo))
    }
}

pub struct Conjunction {
    pub filters: Vec<Box<dyn Filter>>,
}

impl Filter for Conjunction {
    fn filter(&self, todo: &crate::todo::Todo) -> bool {
        self.filters.iter().all(|clause| clause.filter(todo))
    }
}

pub struct Negation {
    pub filter: Box<dyn Filter>,
}

impl Filter for Negation {
    fn filter(&self, todo: &crate::todo::Todo) -> bool {
        !self.filter.filter(todo)
    }
}

#[derive(Debug, PartialEq)]
pub struct All {}

impl Filter for All {
    fn filter(&self, _todo: &crate::todo::Todo) -> bool {
        true
    }
}

pub fn parse_str(filter_def: String) -> Result<Box<dyn Filter>, String> {
    self::parser::parse_expression(&filter_def)
}
