use super::TodoCommand;
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use std::process::ExitCode;

pub const USAGE: &str = r#"Restore the todo store from VCS history

Usage: tdz todo import

Options:
    --help    Print help
"#;

pub struct TodoImportOptions {}

pub fn parse_opts(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use lexopt::prelude::*;

    while let Some(arg) = parser.next()? {
        match arg {
            Long("help") => return Ok(Mode::Help(USAGE)),
            _ => return Err(arg.unexpected().into()),
        }
    }

    Ok(Mode::Cli(Command::Todo(TodoCommand::Import(
        TodoImportOptions {},
    ))))
}

pub fn import(conf: &mut config::Config, _opts: &TodoImportOptions) -> error::Result<ExitCode> {
    let tdz = crate::cli::tdz::Tdz::open(conf)?;
    match tdz.import_todos() {
        Ok(imported) => {
            for (id, title) in &imported {
                println!("Imported: #{} {}", id, title);
            }
            if imported.is_empty() {
                println!("No todos found in VCS history.");
            } else {
                println!("Imported {} todo(s).", imported.len());
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("{}", e);
            Ok(ExitCode::FAILURE)
        }
    }
}
