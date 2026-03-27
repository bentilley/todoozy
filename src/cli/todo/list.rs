use crate::cli::config;
use todoozy::todo::filter;
use todoozy::todo::sort;
use super::OutputFormat;

pub struct TodoListOptions {
    pub limit: Option<usize>,
    pub format: OutputFormat,
    pub filter: Option<Box<dyn filter::Filter>>,
    pub sorter: Option<Box<dyn sort::Sorter>>,
}

impl Default for TodoListOptions {
    fn default() -> Self {
        Self {
            limit: None,
            format: OutputFormat::Table,
            filter: None,
            sorter: None,
        }
    }
}

/// Serializable representation of a Todo for JSON output.
#[derive(serde::Serialize)]
struct TodoOutput {
    id: Option<u32>,
    priority: Option<char>,
    title: String,
    description: Option<String>,
    file: Option<String>,
    line_number: Option<usize>,
    tags: Vec<String>,
}

impl From<&todoozy::todo::Todo> for TodoOutput {
    fn from(todo: &todoozy::todo::Todo) -> Self {
        TodoOutput {
            id: todo.id.as_ref().map(|id| match id {
                todoozy::todo::TodoIdentifier::Primary(n) => *n,
                todoozy::todo::TodoIdentifier::Reference(n) => *n,
            }),
            priority: todo.priority,
            title: todo.title.clone(),
            description: todo.description.clone(),
            file: todo.file.clone(),
            line_number: todo.line_number,
            tags: todo.tags.clone(),
        }
    }
}

pub fn parse_opts(mut parser: lexopt::Parser) -> Result<TodoListOptions, lexopt::Error> {
    use lexopt::prelude::*;

    let mut opts = TodoListOptions::default();

    while let Some(arg) = parser.next()? {
        match arg {
            Short('f') | Long("filter") => opts.filter = Some(parser.value()?.parse()?),
            Long("format") => opts.format = parser.value()?.parse()?,
            Short('n') | Long("limit") => opts.limit = Some(parser.value()?.parse()?),
            Short('s') | Long("sort") => opts.sorter = Some(parser.value()?.parse()?),
            _ => return Err(arg.unexpected()),
        }
    }

    Ok(opts)
}

pub fn list(conf: &config::Config, opts: &TodoListOptions) {
    let mut todos = match todoozy::get_todos(&conf.exclude) {
        Ok(todos) => todos,
        Err(e) => {
            eprintln!("Error loading todos: {}", e);
            return;
        }
    };

    // Use opts.filter if present, otherwise fall back to conf.filter
    if let Some(ref f) = opts.filter {
        todos.apply_filter(|todo| f.filter(todo));
    } else if let Some(ref f) = conf.filter {
        todos.apply_filter(|todo| f.filter(todo));
    }

    // Use opts.sorter if present, otherwise fall back to conf.sorter
    if let Some(ref s) = opts.sorter {
        todos.apply_sort(|a, b| s.compare(a, b));
    } else if let Some(ref s) = conf.sorter {
        todos.apply_sort(|a, b| s.compare(a, b));
    }

    let all_todos: Vec<_> = todos.into();

    // Apply limit if specified
    let all_todos: Vec<_> = match opts.limit {
        Some(n) => all_todos.into_iter().take(n).collect(),
        None => all_todos,
    };

    match opts.format {
        OutputFormat::Table => {
            let id_width = all_todos
                .iter()
                .map(|t| t.display_id().len())
                .max()
                .unwrap_or(0);

            let location_width = all_todos
                .iter()
                .map(|t| t.display_location_start().len())
                .max()
                .unwrap_or(0);

            for todo in all_todos {
                println!(
                    "{:<id_width$} {} {:<location_width$} {}",
                    todo.display_id(),
                    todo.display_priority(),
                    todo.display_location_start(),
                    todo.display_title(),
                )
            }
        }
        OutputFormat::Json => {
            let output: Vec<TodoOutput> = all_todos.iter().map(TodoOutput::from).collect();
            match serde_json::to_string_pretty(&output) {
                Ok(json) => println!("{}", json),
                Err(e) => eprintln!("Error serializing to JSON: {}", e),
            }
        }
    }
}
