pub mod filter;
pub mod parser;
pub mod sort;

use derive_builder::Builder;

#[derive(Builder, Debug, PartialEq, Default)]
pub struct Todo {
    // TODO (A) 2024-09-05 Update id with custom # format +improvement
    //
    // Add the ID to the title format, i.e. "TODO #123 (B) blah blah". If you're syncing to JIRA or
    // GitHub then we'll need to keep the external ID in the metadata.
    //
    // This also requires storing some state about how many todos we've seen. This needs to be done
    // in a file that is also kept under version control. I think a todoozy.yaml file that can also
    // have project config in would be the place for this. When you add new todos, todoozy will
    // number them for you and then increment the number in the file. The user then has to remember
    // to commit the todoozy.yaml file with the new todos.
    #[builder(default)]
    pub id: Option<String>,

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
