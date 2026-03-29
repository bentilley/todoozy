use super::TodoCommand;
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use tempfile::NamedTempFile;

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

pub fn remove(conf: &config::Config, opts: &TodoRemoveOptions) -> error::Result<()> {
    let todo = todoozy::get_todo(opts.id, &conf.exclude)?
        .ok_or_else(|| error::Error::from(format!("Todo #{} not found", opts.id)))?;

    // TODO #75 (C) 2026-03-27 This deletion logic should live on Todo in src/todo
    //
    // The logic for deleting a todo from a file should be encapsulated in the Todo struct or
    // a related module in src/todo. This keeps the CLI code focused on parsing arguments and
    // handling user interaction, while the core logic of managing todos is centralized in one
    // place. It also makes it easier to test the deletion logic independently of the CLI.
    let file_path = todo
        .file
        .as_ref()
        .ok_or_else(|| error::Error::from(format!("Todo #{} has no file location", opts.id)))?;

    let start_line = todo
        .line_number
        .ok_or_else(|| error::Error::from(format!("Todo #{} has no line number", opts.id)))?;

    // end_line_number is inclusive; if not set, it's just the start line
    let end_line = todo.end_line_number.unwrap_or(start_line);

    // Read the file and remove the lines
    let file = File::open(file_path)
        .map_err(|e| error::Error::from(format!("Error opening file '{}': {}", file_path, e)))?;
    let reader = BufReader::new(file);

    let tmp_file = NamedTempFile::new()
        .map_err(|e| error::Error::from(format!("Error creating temp file: {}", e)))?;
    let mut writer = BufWriter::new(tmp_file.as_file());

    for (i, line) in reader.lines().enumerate() {
        let line_num = i + 1; // 1-indexed
        let content = line
            .map_err(|e| error::Error::from(format!("Error reading file: {}", e)))?;
        // Skip lines within the todo range
        if line_num >= start_line && line_num <= end_line {
            continue;
        }
        writeln!(writer, "{}", content)
            .map_err(|e| error::Error::from(format!("Error writing to temp file: {}", e)))?;
    }

    writer
        .flush()
        .map_err(|e| error::Error::from(format!("Error flushing temp file: {}", e)))?;

    std::fs::copy(tmp_file.path(), file_path)
        .map_err(|e| error::Error::from(format!("Error copying temp file to '{}': {}", file_path, e)))?;

    println!("Removed: #{} {}", opts.id, todo.title);

    Ok(())
}
