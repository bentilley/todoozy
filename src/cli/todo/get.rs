use super::OutputFormat;
use crate::cli::config;
use todoozy::todo::TodoIdentifier;

pub struct TodoGetOptions {
    pub id: u32,
    pub format: OutputFormat,
}

pub fn parse_opts(mut parser: lexopt::Parser) -> Result<TodoGetOptions, lexopt::Error> {
    use lexopt::prelude::*;

    let mut id: Option<u32> = None;
    let mut format = OutputFormat::Table;

    while let Some(arg) = parser.next()? {
        match arg {
            Long("format") => format = parser.value()?.parse()?,
            Value(val) if id.is_none() => {
                id = Some(val.parse().map_err(|_| {
                    lexopt::Error::Custom(format!("invalid ID '{}'", val.to_string_lossy()).into())
                })?);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    let id = id.ok_or_else(|| lexopt::Error::Custom("missing ID argument".into()))?;

    Ok(TodoGetOptions { id, format })
}

pub fn get(conf: &config::Config, opts: &TodoGetOptions) {
    let todos = match todoozy::get_todos(&conf.exclude) {
        Ok(todos) => todos,
        Err(e) => {
            eprintln!("Error loading todos: {}", e);
            return;
        }
    };

    let todo = todos
        .iter()
        .find(|t| matches!(&t.id, Some(TodoIdentifier::Primary(id)) if *id == opts.id));

    match todo {
        Some(todo) => match opts.format {
            OutputFormat::Table => print_table(todo),
            OutputFormat::Json => print_json(todo),
        },
        None => {
            eprintln!("Todo #{} not found", opts.id);
            std::process::exit(1);
        }
    }
}

// TODO #72 (E) 2026-03-27 Rename "table" to "raw" in OutputFormat +refactor
//
// Table came from the `todo list` command, but really it should be more generic since it can be
// used for both `get` and `list`. The "table" format is really just a more human-friendly raw
// output, so "raw" might be a better name. This would also make it clearer that the "table" format
// doesn't necessarily have to be a literal table with columns, but can be any human-readable
// format that isn't JSON.
fn print_table(todo: &todoozy::todo::Todo) {
    // ID and priority
    println!("ID:          {}", todo.display_id());
    println!("Priority:    {}", todo.display_priority());

    // Dates
    if let Some(date) = todo.creation_date {
        println!("Created:     {}", date);
    }
    if let Some(date) = todo.completion_date {
        println!("Completed:   {}", date);
    }

    // Location with line range
    match (&todo.file, todo.line_number, todo.end_line_number) {
        (Some(file), Some(start), Some(end)) => {
            println!("Location:    {}:{}-{}", file, start, end)
        }
        (Some(file), Some(line), None) => println!("Location:    {}:{}", file, line),
        (Some(file), None, None) => println!("Location:    {}", file),
        _ => {}
    }

    // Projects and contexts
    if !todo.projects.is_empty() {
        println!(
            "Projects:    {}",
            todo.projects
                .iter()
                .map(|p| format!("+{}", p))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    if !todo.contexts.is_empty() {
        println!(
            "Contexts:    {}",
            todo.contexts
                .iter()
                .map(|c| format!("@{}", c))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    // Title
    println!();
    println!("Title:");
    println!("  {}", todo.title);

    // Description
    if let Some(ref desc) = todo.description {
        println!();
        println!("Description:");
        for line in desc.lines() {
            println!("  {}", line);
        }
    }

    // Metadata
    let metadata: Vec<_> = todo.metadata.iter().collect();
    if !metadata.is_empty() {
        println!();
        println!("Metadata:");
        for (key, value) in metadata {
            println!("  {}: {}", key, value);
        }
    }
}

fn print_json(todo: &todoozy::todo::Todo) {
    #[derive(serde::Serialize)]
    struct TodoFullOutput {
        id: Option<u32>,
        id_type: Option<String>,
        priority: Option<char>,
        creation_date: Option<String>,
        completion_date: Option<String>,
        file: Option<String>,
        line_number: Option<usize>,
        end_line_number: Option<usize>,
        title: String,
        description: Option<String>,
        projects: Vec<String>,
        contexts: Vec<String>,
        metadata: std::collections::HashMap<String, String>,
    }

    let (id, id_type) = match &todo.id {
        Some(TodoIdentifier::Primary(n)) => (Some(*n), Some("primary".to_string())),
        Some(TodoIdentifier::Reference(n)) => (Some(*n), Some("reference".to_string())),
        None => (None, None),
    };

    let output = TodoFullOutput {
        id,
        id_type,
        priority: todo.priority,
        creation_date: todo.creation_date.map(|d| d.to_string()),
        completion_date: todo.completion_date.map(|d| d.to_string()),
        file: todo.file.clone(),
        line_number: todo.line_number,
        end_line_number: todo.end_line_number,
        title: todo.title.clone(),
        description: todo.description.clone(),
        projects: todo.projects.clone(),
        contexts: todo.contexts.clone(),
        metadata: todo
            .metadata
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    };

    match serde_json::to_string_pretty(&output) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("Error serializing to JSON: {}", e),
    }
}
