use super::TodoCommand;
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use std::process::ExitCode;
use todoozy::provider::vcs::create_vcs_backend;
use todoozy::todo::store::{SqliteStore, Store};

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
    let repo = git2::Repository::open_from_env()?;
    let root = repo.workdir().ok_or("Could not find workdir")?;

    let vcs = create_vcs_backend(root, &conf.get_todo_token(), None)?;
    let todos = vcs.get_all_todos()?;
    let store = SqliteStore::new()?;

    let mut imported_count = 0;
    let mut ids: Vec<u32> = todos.ids().collect();
    ids.sort_unstable();

    for id in ids {
        let todo = todos.get(&id).unwrap();
        match store.import_todo(id, todo) {
            Ok(()) => {
                println!("Imported: #{} {}", id, todo.title);
                imported_count += 1;
            }
            Err(e) => {
                eprintln!("Error importing #{}: {}", id, e);
                return Ok(ExitCode::FAILURE);
            }
        }
    }

    if imported_count == 0 {
        println!("No todos found in VCS history.");
    } else {
        println!("Imported {} todo(s).", imported_count);
    }

    Ok(ExitCode::SUCCESS)
}
