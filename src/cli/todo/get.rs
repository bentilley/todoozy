use super::{OutputFormat, TodoCommand};
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use todoozy::todo::TodoIdentifier;

pub const USAGE: &str = r#"Show full details for a specific todo

Usage: tdz todo get [OPTIONS] <ID>

Arguments:
    <ID>    The todo ID to display

Options:
    --format <FORMAT>  Output format: table, json (default: table)
    --help             Print help
"#;

pub struct TodoGetOptions {
    pub id: u32,
    pub format: OutputFormat,
}

pub fn parse_opts(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use lexopt::prelude::*;

    let mut id: Option<u32> = None;
    let mut format = OutputFormat::Table;

    while let Some(arg) = parser.next()? {
        match arg {
            Long("format") => format = parser.value()?.parse()?,
            Long("help") => return Ok(Mode::Help(USAGE)),
            Value(val) if id.is_none() => {
                id = Some(val.parse().map_err(|_| {
                    error::Error::from(format!("invalid ID '{}'", val.to_string_lossy()))
                })?);
            }
            _ => return Err(arg.unexpected().into()),
        }
    }

    let id = id.ok_or_else(|| error::Error::from("missing ID argument"))?;

    Ok(Mode::Cli(Command::Todo(TodoCommand::Get(TodoGetOptions {
        id,
        format,
    }))))
}

pub fn get(conf: &config::Config, opts: &TodoGetOptions) -> error::Result<()> {
    let todo = todoozy::get_todo(opts.id, &conf.exclude)?;

    match todo {
        Some(todo) => match opts.format {
            OutputFormat::Table => print_table(&todo),
            OutputFormat::Json => print_json(&todo),
        },
        None => return Err(format!("Todo #{} not found", opts.id).into()),
    };

    Ok(())
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

    // Location(s) with line range
    if todo.references.is_empty() {
        println!("Location:    {}", todo.location);
    } else {
        // Multiple locations with markers
        println!("Locations:");
        for location in &todo.display_locations_with_marker() {
            println!("             {}", location);
        }
    }

    // Tags (merged from primary + references)
    let merged_tags = todo.display_merged_tags();
    if !merged_tags.is_empty() {
        println!(
            "Tags:        {}",
            merged_tags
                .iter()
                .map(|t| format!("+{}", t))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    // Title
    println!();
    println!("Title:");
    println!("  {}", todo.title);

    // Description (merged with reference subtitles)
    if let Some(ref desc) = todo.display_merged_description() {
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
    struct TodoRefOutput {
        id: Option<u32>,
        file: Option<String>,
        line_number: Option<usize>,
        end_line_number: Option<usize>,
        title: String,
        description: Option<String>,
        tags: Vec<String>,
        metadata: std::collections::HashMap<String, String>,
    }

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
        tags: Vec<String>,
        metadata: std::collections::HashMap<String, String>,
        references: Vec<TodoRefOutput>,
    }

    let (id, id_type) = match &todo.id {
        Some(TodoIdentifier::Primary(n)) => (Some(*n), Some("primary".to_string())),
        Some(TodoIdentifier::Reference(n)) => (Some(*n), Some("reference".to_string())),
        None => (None, None),
    };

    let references: Vec<TodoRefOutput> = todo
        .references
        .iter()
        .map(|r| {
            let ref_id = match &r.id {
                Some(TodoIdentifier::Reference(n)) => Some(*n),
                Some(TodoIdentifier::Primary(n)) => Some(*n),
                None => None,
            };
            TodoRefOutput {
                id: ref_id,
                file: r.location.file_path.clone(),
                line_number: Some(r.location.start_line_num),
                end_line_number: Some(r.location.end_line_num),
                title: r.title.clone(),
                description: r.description.clone(),
                tags: r.tags.clone(),
                metadata: r
                    .metadata
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            }
        })
        .collect();

    let output = TodoFullOutput {
        id,
        id_type,
        priority: todo.priority,
        creation_date: todo.creation_date.map(|d| d.to_string()),
        completion_date: todo.completion_date.map(|d| d.to_string()),
        file: todo.location.file_path.clone(),
        line_number: Some(todo.location.start_line_num),
        end_line_number: Some(todo.location.end_line_num),
        title: todo.title.clone(),
        description: todo.description.clone(),
        tags: todo.tags.clone(),
        metadata: todo
            .metadata
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        references,
    };

    match serde_json::to_string_pretty(&output) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("Error serializing to JSON: {}", e),
    }
}
