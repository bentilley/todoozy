use super::TodoCommand;
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use tempfile::NamedTempFile;
use todoozy::todo::TodoIdentifier;

pub const USAGE: &str = r#"Delete a todo comment from its source file

Usage: tdz todo remove <ID>

Arguments:
    <ID>    The todo ID to remove

Options:
    --help  Print help

Note: This permanently deletes the TODO comment from the source file.
"#;

pub struct TodoRemoveOptions {
    pub id: u32,
}

pub fn parse_opts(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use lexopt::prelude::*;

    let mut id: Option<u32> = None;

    while let Some(arg) = parser.next()? {
        match arg {
            Long("help") => return Ok(Mode::Help(USAGE)),
            Value(val) if id.is_none() => {
                id = Some(
                    val.parse()
                        .map_err(|_| error::Error::from(format!("invalid ID '{}'", val.to_string_lossy())))?,
                );
            }
            _ => return Err(arg.unexpected().into()),
        }
    }

    let id = id.ok_or_else(|| error::Error::from("missing ID argument"))?;

    Ok(Mode::Cli(Command::Todo(TodoCommand::Remove(TodoRemoveOptions { id }))))
}

// TODO #74 (C) 2026-03-27 Lift these exits up to src/main.rs
//
// Functions should end with a Result<(), Error> and let main.rs handle the exit codes. This allows
// for better error handling, cleanup, and testing.
pub fn remove(conf: &config::Config, opts: &TodoRemoveOptions) {
    let todos = match todoozy::get_todos(&conf.exclude) {
        Ok(todos) => todos,
        Err(e) => {
            eprintln!("Error loading todos: {}", e);
            std::process::exit(1);
        }
    };

    let todo = todos
        .iter()
        .find(|t| matches!(&t.id, Some(TodoIdentifier::Primary(id)) if *id == opts.id));

    let todo = match todo {
        Some(t) => t,
        None => {
            eprintln!("Todo #{} not found", opts.id);
            std::process::exit(1);
        }
    };

    // TODO #75 (C) 2026-03-27 This deletion logic should live on Todo in src/todo
    //
    // The logic for deleting a todo from a file should be encapsulated in the Todo struct or
    // a related module in src/todo. This keeps the CLI code focused on parsing arguments and
    // handling user interaction, while the core logic of managing todos is centralized in one
    // place. It also makes it easier to test the deletion logic independently of the CLI.
    let file_path = match &todo.file {
        Some(f) => f,
        None => {
            eprintln!("Todo #{} has no file location", opts.id);
            std::process::exit(1);
        }
    };

    let start_line = match todo.line_number {
        Some(l) => l,
        None => {
            eprintln!("Todo #{} has no line number", opts.id);
            std::process::exit(1);
        }
    };

    // end_line_number is inclusive; if not set, it's just the start line
    let end_line = todo.end_line_number.unwrap_or(start_line);

    // Read the file and remove the lines
    let file = match File::open(file_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error opening file '{}': {}", file_path, e);
            std::process::exit(1);
        }
    };
    let reader = BufReader::new(file);

    let tmp_file = match NamedTempFile::new() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error creating temp file: {}", e);
            std::process::exit(1);
        }
    };
    let mut writer = BufWriter::new(tmp_file.as_file());

    for (i, line) in reader.lines().enumerate() {
        let line_num = i + 1; // 1-indexed
        match line {
            Ok(content) => {
                // Skip lines within the todo range
                if line_num >= start_line && line_num <= end_line {
                    continue;
                }
                if let Err(e) = writeln!(writer, "{}", content) {
                    eprintln!("Error writing to temp file: {}", e);
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Error reading file: {}", e);
                std::process::exit(1);
            }
        }
    }

    if let Err(e) = writer.flush() {
        eprintln!("Error flushing temp file: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = std::fs::copy(tmp_file.path(), file_path) {
        eprintln!("Error copying temp file to '{}': {}", file_path, e);
        std::process::exit(1);
    }

    println!("Removed: #{} {}", opts.id, todo.title);
}
