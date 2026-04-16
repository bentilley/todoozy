use super::{OutputFormat, TodoCommand};
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use todoozy::provider::{FileSystemProvider, Provider};
use todoozy::todo::filter;
use todoozy::todo::sort;

// TODO #81 (A) 2026-04-16 Support completed todos in `tdz todo list`
//
// Currently, `tdz todo list` only shows active todos. We should add an option to include completed
// todos as well, and display their completion status in the output. This will require using the
// VcsBackend provider in src/provider/vcs to get completed todos from the history.
//
// Add an --all flag to toggle whether the command shows the completed todos or not.
//
// If the user provides the --all flag, then I think the process is to load the vcs todos, then
// load the current todos, override the vcs todos with current ones so that we have the most up to
// date info and then proceed with filtering / sorting / etc. as before.
pub const USAGE: &str = r#"List todos in compact table format

Usage: tdz todo list [OPTIONS]

Options:
    -n, --limit <N>         Limit number of results
    -f, --filter <FILTER>   Filter which todos to display
    -s, --sort <SORT>       How to sort the todos
    --format <FORMAT>       Output format: raw, json (default: raw)
    --help                  Print help
"#;

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
            format: OutputFormat::Raw,
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
            file: todo.location.file_path.clone(),
            line_number: Some(todo.location.start_line_num),
            tags: todo.tags.clone(),
        }
    }
}

pub fn parse_opts(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use lexopt::prelude::*;

    let mut opts = TodoListOptions::default();

    while let Some(arg) = parser.next()? {
        match arg {
            Short('f') | Long("filter") => opts.filter = Some(parser.value()?.parse()?),
            Long("format") => opts.format = parser.value()?.parse()?,
            Short('n') | Long("limit") => opts.limit = Some(parser.value()?.parse()?),
            Short('s') | Long("sort") => opts.sorter = Some(parser.value()?.parse()?),
            Long("help") => return Ok(Mode::Help(USAGE)),
            _ => return Err(arg.unexpected().into()),
        }
    }

    Ok(Mode::Cli(Command::Todo(TodoCommand::List(opts))))
}

pub fn list(conf: &config::Config, opts: &TodoListOptions) -> error::Result<()> {
    let mut todos =
        FileSystemProvider::new(&conf.get_todo_token(), conf.exclude.clone()).get_todos()?;

    // Use opts.filter if present, otherwise fall back to conf.filter
    if let Some(ref f) = opts.filter {
        todos.apply_filter(|todo| f.filter(todo));
    } else if let Some(ref f) = conf.filter {
        todos.apply_filter(|todo| f.filter(todo));
    }

    // Use opts.sorter if present, otherwise fall back to conf.sorter
    let sorter = opts.sorter.as_ref().or(conf.sorter.as_ref());

    // Convert to Vec, applying sort if specified
    let all_todos: Vec<_> = match sorter {
        Some(s) => todos.into_sorted(|a, b| s.compare(a, b)),
        None => todos.into(),
    };

    // Apply limit if specified
    let all_todos: Vec<_> = match opts.limit {
        Some(n) => all_todos.into_iter().take(n).collect(),
        None => all_todos,
    };

    match opts.format {
        OutputFormat::Raw => {
            let id_width = all_todos
                .iter()
                .map(|t| t.display_id().len())
                .max()
                .unwrap_or(0);

            let location_width = all_todos
                .iter()
                .map(|t| t.location.display_start().len())
                .max()
                .unwrap_or(0);

            for todo in all_todos {
                println!(
                    "{:<id_width$} {} {:<location_width$} {} {}",
                    todo.display_id(),
                    todo.display_priority(),
                    todo.location.display_start(),
                    todo.title,
                    todo.display_tags(),
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

    Ok(())
}
