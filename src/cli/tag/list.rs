use super::TagCommand;
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use std::collections::HashMap;

pub const USAGE: &str = r#"List all tags with counts

Usage: tdz tag list [OPTIONS]

Options:
    -n, --limit <N>       Limit number of results
    -s, --sort <SORT>     Sort order: name, count (default: name)
    --format <FORMAT>     Output format: table, json (default: table)
    --help                Print help
"#;

pub struct TagListOptions {
    pub limit: Option<usize>,
    pub format: OutputFormat,
    pub sort: SortOrder,
}

impl Default for TagListOptions {
    fn default() -> Self {
        Self {
            limit: None,
            format: OutputFormat::Table,
            sort: SortOrder::Name,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "table" => Ok(OutputFormat::Table),
            "json" => Ok(OutputFormat::Json),
            other => Err(format!(
                "unknown format '{}', expected 'table' or 'json'",
                other
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    #[default]
    Name,
    Count,
}

impl std::str::FromStr for SortOrder {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "name" => Ok(SortOrder::Name),
            "count" => Ok(SortOrder::Count),
            other => Err(format!(
                "unknown sort '{}', expected 'name' or 'count'",
                other
            )),
        }
    }
}

#[derive(serde::Serialize)]
struct TagOutput {
    name: String,
    count: usize,
}

pub fn parse_opts(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use lexopt::prelude::*;

    let mut opts = TagListOptions::default();

    while let Some(arg) = parser.next()? {
        match arg {
            Long("format") => opts.format = parser.value()?.parse()?,
            Short('n') | Long("limit") => opts.limit = Some(parser.value()?.parse()?),
            Short('s') | Long("sort") => opts.sort = parser.value()?.parse()?,
            Long("help") => return Ok(Mode::Help(USAGE)),
            _ => return Err(arg.unexpected().into()),
        }
    }

    Ok(Mode::Cli(Command::Tag(TagCommand::List(opts))))
}

pub fn list(conf: &config::Config, opts: &TagListOptions) {
    let todos = match todoozy::get_todos(&conf.exclude) {
        Ok(todos) => todos,
        Err(e) => {
            eprintln!("Error loading todos: {}", e);
            return;
        }
    };

    // Collect tags with counts
    let mut tags: HashMap<String, usize> = HashMap::new();
    for todo in todos {
        for tag in todo.tags {
            *tags.entry(tag).or_insert(0) += 1;
        }
    }

    // Convert to vec for sorting
    let mut tags: Vec<(String, usize)> = tags.into_iter().collect();

    // Apply sort
    match opts.sort {
        SortOrder::Name => tags.sort_by(|a, b| a.0.cmp(&b.0)),
        SortOrder::Count => tags.sort_by(|a, b| b.1.cmp(&a.1)),
    }

    // Apply limit if specified
    let tags: Vec<_> = match opts.limit {
        Some(n) => tags.into_iter().take(n).collect(),
        None => tags,
    };

    match opts.format {
        OutputFormat::Table => {
            let name_width = tags.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
            let count_width = tags
                .iter()
                .map(|(_, count)| count.to_string().len())
                .max()
                .unwrap_or(0);

            for (name, count) in tags {
                println!("{:<name_width$} {:>count_width$}", name, count);
            }
        }
        OutputFormat::Json => {
            let output: Vec<TagOutput> = tags
                .into_iter()
                .map(|(name, count)| TagOutput { name, count })
                .collect();
            match serde_json::to_string_pretty(&output) {
                Ok(json) => println!("{}", json),
                Err(e) => eprintln!("Error serializing to JSON: {}", e),
            }
        }
    }
}
