use core::fmt::{self, Display, Formatter};
use derive_builder::Builder;

#[derive(Builder, Debug, PartialEq, Default)]
pub struct Todo {
    #[builder(default)]
    pub file: Option<String>,
    #[builder(default)]
    pub line_number: Option<usize>,

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

// TDZ (A) 2024-08-15 Also display the dates, projects, contexts, and metadata. +features
//
// Ultimately this probably shouldn't live here (want to separate the data from the display logic),
// but fine for now to get an MVP.
// ZDT
impl Display for Todo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.title)?;

        if let Some(ref file) = self.file {
            if let Some(line_number) = self.line_number {
                write!(f, " ({}:{})", file, line_number)?;
            } else {
                write!(f, " ({})", file)?;
            }
        }

        if let Some(ref description) = self.description {
            write!(f, "\n\n{}", description)
        } else {
            Ok(())
        }
    }
}
