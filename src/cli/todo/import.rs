use super::TodoCommand;
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;

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
    let todos = todoozy::get_todos(&conf.exclude)?;

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
