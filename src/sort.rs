pub trait Sorter {
    fn compare(&self, a: &crate::todo::Todo, b: &crate::todo::Todo) -> std::cmp::Ordering;
}

pub struct Priority {}

impl Sorter for Priority {
    fn compare(&self, a: &crate::todo::Todo, b: &crate::todo::Todo) -> std::cmp::Ordering {
        let a = a.priority.unwrap_or('Z');
        let b = b.priority.unwrap_or('Z');
        if a < b {
            std::cmp::Ordering::Less
        } else if a > b {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    }
}

pub fn parse_str(_sort_def: String) -> Box<dyn Sorter> {
    Box::new(Priority {})
}
