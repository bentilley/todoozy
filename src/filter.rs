mod parser;

pub trait Filter {
    fn filter(&self, todo: &crate::todo::Todo) -> bool;
}

// TODO (C) 2024-08-22 Add creation date and completion date filters +feature
//
// Non-trivial as this would require adding > and < comparators to the mini-query language.
// +parsing +nom
// ODOT
#[derive(Debug, PartialEq)]
enum FilterProperty {
    File,
    Priority,
    Project,
    Context,
}

// TODO (B) 2024-08-22 Add some "operation" property to the PropertyFilter +feature
//
// This would allow us to specify a filter like "priority > A" or "priority < B" or "priority = C".
// This will be needed for the creation date and completion date filters too, but just realised
// that it's also super useful for priority!
// ODOT
// TODO (B) 2024-08-22 Add a "not" operation to the PropertyFilter +feature
#[derive(Debug, PartialEq)]
pub struct PropertyFilter {
    property: FilterProperty,
    pub value: String,
}

impl Filter for PropertyFilter {
    fn filter(&self, todo: &crate::todo::Todo) -> bool {
        match self.property {
            FilterProperty::File => todo.file == Some(self.value.clone()),
            FilterProperty::Priority => todo.priority == Some(self.value.chars().next().unwrap()),
            FilterProperty::Project => todo.has_project(&self.value),
            FilterProperty::Context => todo.has_context(&self.value),
        }
    }
}

pub struct DisjunctionFilter {
    pub filters: Vec<Box<dyn Filter>>,
}

impl Filter for DisjunctionFilter {
    fn filter(&self, todo: &crate::todo::Todo) -> bool {
        self.filters.iter().any(|clause| clause.filter(todo))
    }
}

pub struct ConjunctionFilter {
    pub filters: Vec<Box<dyn Filter>>,
}

impl Filter for ConjunctionFilter {
    fn filter(&self, todo: &crate::todo::Todo) -> bool {
        self.filters.iter().all(|clause| clause.filter(todo))
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
