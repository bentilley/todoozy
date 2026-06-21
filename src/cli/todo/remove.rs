use std::process::ExitCode;

use super::TodoCommand;
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use crate::cli::tdz::TodoID;

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
                id = Some(val.parse().map_err(|_| {
                    error::Error::from(format!("invalid ID '{}'", val.to_string_lossy()))
                })?);
            }
            _ => return Err(arg.unexpected().into()),
        }
    }

    let id = id.ok_or_else(|| error::Error::from("missing ID argument"))?;

    Ok(Mode::Cli(Command::Todo(TodoCommand::Remove(
        TodoRemoveOptions { id },
    ))))
}

pub fn remove(conf: &config::Config, opts: &TodoRemoveOptions) -> error::Result<ExitCode> {
    let tdz = crate::cli::tdz::Tdz::open(conf)?;
    let todo = tdz.remove_todo(&TodoID::Legacy(opts.id))?;

    println!("Removed: #{} {}", opts.id, todo.title);

    Ok(ExitCode::SUCCESS)
}
