pub mod parser;

use derive_builder::Builder;

#[derive(Builder, Debug, PartialEq, Default)]
pub struct Todo {
    // TODO (A) 2024-09-05 This ID needs some thought +improvement
    //
    // I'm not sure if this is the right place for it, maybe it should just be a token that appears
    // after the name, e.g. "TODO 123 (B) blah blah". If you were syncing these with JIRA or
    // Github, then you would probably want their ID in a prominent place. Also, this way it
    // doesn't take up an extra line. And would make programatically adding it to the code easier
    // as you know it just needs to go between the "TODO" and the next token. We could prepend it
    // with a "#" to make it easier to pick out by eye when scanning?
    //
    // The issue that this is bringing up for me is, what happens when you start syncing with an
    // external system. If you start using todoozy without, then you'd had some native todoozy IDs,
    // then you sync to JIRA and it's going to create a bunch of JIRA IDs - where do they go? Then
    // what happens if you want to sync with JIRA AND GitHub, you get another set of IDs from each
    // additional backend. This is where the metadata approach would shine as you could have
    // _jira_id:SOME-123 and _github_id:456 and everything still plays nice.
    //
    // This makes me think we need a todoozy ID AND then separate metadata IDs for each backend
    // (snore). However, there is still the question then of how to keep track of each todoozy ID
    // and ensure it's unique. If we used an incrementing number, al la Github, then we'd need to
    // do a full todo history scan each time we need a new ID to make sure that we're not re using
    // IDs that were already commited to the history. If we just use a UUID, then it's super long
    // and going to take up a bunch of todo real estate.
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
