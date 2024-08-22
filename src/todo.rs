use derive_builder::Builder;

#[derive(Builder, Debug, PartialEq, Default)]
pub struct Todo {
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
    pub fn has_project(&self, project: &str) -> bool {
        self.projects.iter().any(|p| p == project)
    }

    pub fn has_context(&self, context: &str) -> bool {
        self.contexts.iter().any(|c| c == context)
    }
}
