use super::TodoCommand;
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use todoozy::provider::{FileSystemProvider, Provider};

pub const USAGE: &str = r#"Import untracked todos (assign IDs)

Usage: tdz todo import [OPTIONS]

Options:
    --all                      Import all untracked todos
    --location <FILE[:LINE]>   Import todo at specific location
    --help                     Print help

Examples:
    tdz todo import --all
    tdz todo import --location src/main.rs
    tdz todo import --location src/main.rs:42
"#;

pub struct TodoImportOptions {
    pub all: bool,
    pub location: Option<LocationSpec>,
}

// TODO #93 (C) 2026-04-16 Support Dir type to import all in directory
//
// This would require changing the LocationSpec enum to include a Dir variant, and updating the
// import logic to handle it. The Dir variant would specify a directory path, and the import logic
// would need to recursively search for todos in that directory and its subdirectories. This would
// allow users to easily import all untracked todos from a specific directory, which could be
// useful for larger projects with many files.
pub enum LocationSpec {
    File(String),
    FileLine(String, usize),
}

impl TodoImportOptions {
    pub fn new() -> Self {
        Self {
            all: false,
            location: None,
        }
    }
}

pub fn parse_opts(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use lexopt::prelude::*;

    let mut opts = TodoImportOptions::new();

    while let Some(arg) = parser.next()? {
        match arg {
            Long("all") => opts.all = true,
            Long("location") => {
                let value: String = parser.value()?.parse()?;
                opts.location = Some(parse_location(&value)?);
            }
            Long("help") => return Ok(Mode::Help(USAGE)),
            _ => return Err(arg.unexpected().into()),
        }
    }

    if !opts.all && opts.location.is_none() {
        return Err("must specify --all or --location <file[:line]>".into());
    }

    if opts.all && opts.location.is_some() {
        return Err("cannot specify both --all and --location".into());
    }

    Ok(Mode::Cli(Command::Todo(TodoCommand::Import(opts))))
}

fn parse_location(value: &str) -> error::Result<LocationSpec> {
    if let Some((file, line_str)) = value.rsplit_once(':') {
        if let Ok(line) = line_str.parse::<usize>() {
            return Ok(LocationSpec::FileLine(file.to_string(), line));
        }
    }
    Ok(LocationSpec::File(value.to_string()))
}

pub fn import(conf: &mut config::Config, opts: &TodoImportOptions) -> error::Result<()> {
    let todos = FileSystemProvider::new(&conf.get_todo_token(), conf.exclude.clone()).get_todos()?;

    let mut imported_count = 0;

    for mut todo in todos {
        if todo.id.is_some() {
            continue;
        }

        // Apply location filter if specified
        if let Some(ref location) = opts.location {
            match location {
                LocationSpec::File(file) => {
                    if todo.location.file_path != Some(file.to_string()) {
                        continue;
                    }
                }
                LocationSpec::FileLine(file, line) => {
                    if todo.location.file_path != Some(file.to_string()) {
                        continue;
                    }
                    if todo.location.start_line_num != *line {
                        continue;
                    }
                }
            }
        }

        conf.num_todos += 1;
        let id = conf.num_todos;

        match todo.import(id) {
            Ok(_) => {
                println!("Imported: #{} {}", id, todo.title);
                imported_count += 1;
            }
            Err(e) => {
                eprintln!("Error importing '{}': {}", todo.title, e);
                conf.num_todos -= 1; // Roll back
            }
        }
    }

    if imported_count > 0 {
        if let Err(e) = conf.save() {
            eprintln!("Error saving config: {}", e);
        }
    }

    if imported_count == 0 {
        println!("No untracked todos found matching the criteria.");
    }

    Ok(())
}
