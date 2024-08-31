mod parser;

pub trait Sorter {
    fn compare(&self, a: &crate::todo::Todo, b: &crate::todo::Todo) -> std::cmp::Ordering;
}

#[derive(Debug, PartialEq)]
enum Property {
    Title,
    File,
    LineNumber,
    Priority,
    CreationDate,
    CompletionDate,
    // TODO (Z) 2024-08-31 Can you sort by project and context? ODOT
    // Project,
    // Context,
}

#[derive(Debug, PartialEq)]
enum Direction {
    Ascending,
    Descending,
}

#[derive(Debug, PartialEq)]
pub struct PropertySorter {
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
            Property::File => a.file.cmp(&b.file),
            Property::LineNumber => a.line_number.cmp(&b.line_number),
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
}

#[derive(Default)]
struct SortPipeline {
    sorters: Vec<Box<dyn Sorter>>,
}

impl SortPipeline {
    fn new(sorters: Vec<Box<dyn Sorter>>) -> Self {
        SortPipeline { sorters }
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
}

pub fn parse_str(sort_def: String) -> Result<Box<dyn Sorter>, String> {
    self::parser::parse_expression(&sort_def)
}
