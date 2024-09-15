pub mod filter;
pub mod parser;
pub mod sort;

use derive_builder::Builder;

#[derive(Builder, Debug, PartialEq, Default)]
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
    pub metadata: std::collections::HashMap<String, String>,
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
