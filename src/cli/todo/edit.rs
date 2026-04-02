use super::TodoCommand;
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use todoozy::provider::{FileSystemProvider, Provider};

pub const USAGE: &str = r#"Open todo in $EDITOR at its file location

Usage: tdz todo edit <ID>

Arguments:
    <ID>    The todo ID to edit

Options:
    --help  Print help

Environment:
    EDITOR  Editor to use (falls back to VISUAL, then vi)
"#;

pub struct TodoEditOptions {
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

    Ok(Mode::Cli(Command::Todo(TodoCommand::Edit(
        TodoEditOptions { id },
    ))))
}

pub fn edit(conf: &config::Config, opts: &TodoEditOptions) -> error::Result<()> {
    let todo = FileSystemProvider::new(&conf.get_todo_token(), conf.exclude.clone())
        .get_todo(opts.id)?
        .ok_or_else(|| error::Error::from(format!("Todo #{} not found", opts.id)))?;

    let editor_cmd = todo
        .editor_command()
        .map_err(|e| error::Error::from(format!("{}", e)))?;

    editor_cmd
        .execute()
        .map_err(|e| error::Error::from(format!("{}", e)))?;

    Ok(())
}
