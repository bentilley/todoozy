use crate::cli::config;
use todoozy::todo::TodoIdentifier;

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

pub fn parse_opts(mut parser: lexopt::Parser) -> Result<TodoImportOptions, lexopt::Error> {
    use lexopt::prelude::*;

    let mut opts = TodoImportOptions::new();

    while let Some(arg) = parser.next()? {
        match arg {
            Long("all") => opts.all = true,
            Long("location") => {
                let value: String = parser.value()?.parse()?;
                opts.location = Some(parse_location(&value)?);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    if !opts.all && opts.location.is_none() {
        return Err(lexopt::Error::Custom(
            "must specify --all or --location <file[:line]>".into(),
        ));
    }

    if opts.all && opts.location.is_some() {
        return Err(lexopt::Error::Custom(
            "cannot specify both --all and --location".into(),
        ));
    }

    Ok(opts)
}

fn parse_location(value: &str) -> Result<LocationSpec, lexopt::Error> {
    if let Some((file, line_str)) = value.rsplit_once(':') {
        if let Ok(line) = line_str.parse::<usize>() {
            return Ok(LocationSpec::FileLine(file.to_string(), line));
        }
    }
    Ok(LocationSpec::File(value.to_string()))
}

pub fn import(conf: &mut config::Config, opts: &TodoImportOptions) {
    let todos = match todoozy::get_todos(&conf.exclude) {
        Ok(todos) => todos,
        Err(e) => {
            eprintln!("Error loading todos: {}", e);
            return;
        }
    };

    let mut imported_count = 0;

    for mut todo in todos {
        if todo.id.is_some() {
            continue;
        }

        // Apply location filter if specified
        if let Some(ref location) = opts.location {
            match location {
                LocationSpec::File(file) => {
                    if todo.file.as_deref() != Some(file.as_str()) {
                        continue;
                    }
                }
                LocationSpec::FileLine(file, line) => {
                    if todo.file.as_deref() != Some(file.as_str()) {
                        continue;
                    }
                    if todo.line_number != Some(*line) {
                        continue;
                    }
                }
            }
        }

        // Assign new ID
        conf.num_todos += 1;
        let id = conf.num_todos;
        todo.id = Some(TodoIdentifier::Primary(id));

        match todo.write_id() {
            Ok(_) => {
                println!("Imported: #{} {}", id, todo.title);
                imported_count += 1;
            }
            Err(e) => {
                eprintln!("Error writing ID for '{}': {}", todo.title, e);
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
}
