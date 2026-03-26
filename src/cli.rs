pub mod args;
pub mod config;
pub mod display;
pub mod tui;

use todoozy::todo::filter;
use todoozy::todo::sort;

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
            other => Err(format!("unknown format '{}', expected 'table' or 'json'", other)),
        }
    }
}

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

pub enum TodoCommand {
    List(TodoListOptions),
}

pub enum Command {
    ListProjects,
    ListContexts,
    ImportAll,
    Todo(TodoCommand),
}

pub fn list_projects(exclude: &[String]) {
    let todos = todoozy::get_todos(exclude).unwrap();
    let mut projects = std::collections::HashMap::new();
    for todo in todos {
        for project in todo.projects {
            let count = projects.entry(project).or_insert(0);
            *count += 1;
        }
    }
    for (project, _) in projects {
        println!("{}", project);
    }
}

pub fn list_contexts(exclude: &[String]) {
    let todos = todoozy::get_todos(exclude).unwrap();
    let mut contexts = std::collections::HashMap::new();
    for todo in todos {
        for context in todo.contexts {
            let count = contexts.entry(context).or_insert(0);
            *count += 1;
        }
    }
    for (context, _) in contexts {
        println!("{}", context);
    }
}

pub fn import_all(conf: &mut config::Config) -> Result<(), Box<dyn std::error::Error>> {
    let todos = todoozy::get_todos(&conf.exclude).unwrap();
    for mut todo in todos {
        match todo.id {
            Some(_) => {}
            None => {
                conf.num_todos += 1;
                let id = conf.num_todos;
                todo.id = Some(todoozy::todo::TodoIdentifier::Primary(id));
                todo.write_id()?;
                println!("Imported: #{} {}", id, todo.title);
            }
        };
    }
    conf.save()?;
    Ok(())
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
    projects: Vec<String>,
    contexts: Vec<String>,
}

impl From<&todoozy::todo::Todo> for TodoOutput {
    fn from(todo: &todoozy::todo::Todo) -> Self {
        TodoOutput {
            id: todo.id,
            priority: todo.priority,
            title: todo.title.clone(),
            description: todo.description.clone(),
            file: todo.file.clone(),
            line_number: todo.line_number,
            projects: todo.projects.clone(),
            contexts: todo.contexts.clone(),
        }
    }
}

pub fn todo_list(conf: &config::Config, opts: &TodoListOptions) {
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
