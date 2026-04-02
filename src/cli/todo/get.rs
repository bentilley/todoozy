use super::{OutputFormat, TodoCommand};
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use todoozy::todo::TodoIdentifier;
use todoozy::provider::{FileSystemProvider, Provider};

pub const USAGE: &str = r#"Show full details for a specific todo

Usage: tdz todo get [OPTIONS] <ID>

Arguments:
    <ID>    The todo ID to display

Options:
    --format <FORMAT>  Output format: raw, json (default: raw)
    --help             Print help
"#;

pub struct TodoGetOptions {
    pub id: u32,
    pub format: OutputFormat,
}

pub fn parse_opts(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use lexopt::prelude::*;

    let mut id: Option<u32> = None;
    let mut format = OutputFormat::Raw;

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
    let todo = FileSystemProvider::new(&conf.get_todo_token(), conf.exclude.clone())
        .get_todo(opts.id)?;

    match todo {
        Some(todo) => match opts.format {
            OutputFormat::Raw => print_raw(&todo),
            OutputFormat::Json => print_json(&todo),
        },
        None => return Err(format!("Todo #{} not found", opts.id).into()),
    };

    Ok(())
}

fn print_raw(todo: &todoozy::todo::Todo) {
    write_raw(&mut std::io::stdout(), todo).unwrap();
}

fn write_raw(w: &mut impl std::io::Write, todo: &todoozy::todo::Todo) -> std::io::Result<()> {
    // ID and priority
    writeln!(w, "ID:          {}", todo.display_id())?;
    writeln!(w, "Priority:    {}", todo.display_priority())?;

    // Dates
    if let Some(date) = todo.creation_date {
        writeln!(w, "Created:     {}", date)?;
    }
    if let Some(date) = todo.completion_date {
        writeln!(w, "Completed:   {}", date)?;
    }

    // Location(s) with line range
    if todo.references.is_empty() {
        writeln!(w, "Location:    {}", todo.location)?;
    } else {
        // Multiple locations with markers
        writeln!(w, "Locations:")?;
        for location in &todo.display_locations_with_marker() {
            writeln!(w, "             {}", location)?;
        }
    }

    // Tags (merged from primary + references)
    let merged_tags = todo.display_merged_tags();
    if !merged_tags.is_empty() {
        writeln!(
            w,
            "Tags:        {}",
            merged_tags
                .iter()
                .map(|t| format!("+{}", t))
                .collect::<Vec<_>>()
                .join(" ")
        )?;
    }

    // Title
    writeln!(w)?;
    writeln!(w, "Title:")?;
    writeln!(w, "  {}", todo.title)?;

    // Description (merged with reference subtitles)
    if let Some(ref desc) = todo.display_merged_description() {
        writeln!(w)?;
        writeln!(w, "Description:")?;
        for line in desc.lines() {
            writeln!(w, "  {}", line)?;
        }
    }

    // Metadata
    if !todo.metadata.is_empty() {
        writeln!(w)?;
        writeln!(w, "Metadata:")?;
        for (key, values) in todo.metadata.iter() {
            writeln!(w, "  {}: {}", key, values)?;
        }
    }

    Ok(())
}

fn print_json(todo: &todoozy::todo::Todo) {
    write_json(&mut std::io::stdout(), todo).unwrap();
}

#[derive(serde::Serialize)]
struct TodoRefOutput {
    id: Option<u32>,
    file: Option<String>,
    line_number: Option<usize>,
    end_line_number: Option<usize>,
    title: String,
    description: Option<String>,
    tags: Vec<String>,
    metadata: std::collections::HashMap<String, Vec<String>>,
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
    metadata: std::collections::HashMap<String, Vec<String>>,
    references: Vec<TodoRefOutput>,
}

fn write_json(w: &mut impl std::io::Write, todo: &todoozy::todo::Todo) -> std::io::Result<()> {
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
                    .keys()
                    .map(|k| (k.clone(), r.metadata.get(k).unwrap().to_vec()))
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
            .keys()
            .map(|k| (k.clone(), todo.metadata.get(k).unwrap().to_vec()))
            .collect(),
        references,
    };

    serde_json::to_writer_pretty(&mut *w, &output)?;
    writeln!(w)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use todoozy::todo::{TodoInfoBuilder, Location, Metadata, Todo, TodoIdentifier};

    #[test]
    fn test_write_raw_format() {
        let mut metadata = Metadata::new();
        metadata.set("depends", "42");
        metadata.set("depends", "41");
        metadata.set("owner", "alice");

        let todo = Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Primary(99)))
                .priority(Some('A'))
                .title("Test todo title".to_string())
                .description(Some("Test description".to_string()))
                .tags(vec!["feature".to_string(), "urgent".to_string()])
                .metadata(metadata)
                .build()
                .unwrap(),
            Location::new(Some("src/main.rs".to_string()), 10, 15),
        );

        let mut buf = Vec::new();
        write_raw(&mut buf, &todo).unwrap();
        let output = String::from_utf8(buf).unwrap();

        // Check basic fields
        assert!(output.contains("ID:          #99"));
        assert!(output.contains("Priority:    (A)"));
        assert!(output.contains("Location:    src/main.rs:10-15"));
        assert!(output.contains("Tags:        +feature +urgent"));
        assert!(output.contains("Title:"));
        assert!(output.contains("  Test todo title"));
        assert!(output.contains("Description:"));
        assert!(output.contains("  Test description"));

        // Check multi-value metadata is displayed correctly
        assert!(output.contains("Metadata:"));
        assert!(output.contains("depends: 42"));
        assert!(output.contains("depends: 41"));
        assert!(output.contains("owner: alice"));
    }

    #[test]
    fn test_write_json_format() {
        let mut metadata = Metadata::new();
        metadata.set("depends", "42");
        metadata.set("depends", "41");
        metadata.set("owner", "alice");

        let todo = Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Primary(99)))
                .priority(Some('A'))
                .title("Test todo title".to_string())
                .description(Some("Test description".to_string()))
                .tags(vec!["feature".to_string(), "urgent".to_string()])
                .metadata(metadata)
                .build()
                .unwrap(),
            Location::new(Some("src/main.rs".to_string()), 10, 15),
        );

        let mut buf = Vec::new();
        write_json(&mut buf, &todo).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        // Check basic fields
        assert_eq!(parsed["id"], 99);
        assert_eq!(parsed["id_type"], "primary");
        assert_eq!(parsed["priority"], "A");
        assert_eq!(parsed["title"], "Test todo title");
        assert_eq!(parsed["description"], "Test description");
        assert_eq!(parsed["file"], "src/main.rs");
        assert_eq!(parsed["line_number"], 10);
        assert_eq!(parsed["end_line_number"], 15);

        // Check tags array
        assert_eq!(parsed["tags"], serde_json::json!(["feature", "urgent"]));

        // Check multi-value metadata is serialized as arrays
        assert_eq!(
            parsed["metadata"]["depends"],
            serde_json::json!(["42", "41"])
        );
        assert_eq!(parsed["metadata"]["owner"], serde_json::json!(["alice"]));
    }
}
