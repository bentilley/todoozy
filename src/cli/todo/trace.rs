use super::TodoCommand;
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use std::process::ExitCode;
use todoozy::provider::vcs::CommitMetadata;
use todoozy::provider::{vcs, FileSystemProvider, Provider};
use todoozy::todo::Todo;

pub const USAGE: &str = r#"Show the commit history of a todo

Usage: tdz todo trace <ID>

Arguments:
    <ID>    The todo ID to trace

Options:
    --help    Print help
"#;

pub struct TodoTraceOptions {
    pub id: u32,
}

pub fn parse_opts(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use lexopt::prelude::*;

    let mut id: Option<u32> = None;

    while let Some(arg) = parser.next()? {
        match arg {
            Long("help") => return Ok(Mode::Help(USAGE)),
            Value(val) if id.is_none() => {
                id = Some(val.parse()?);
            }
            _ => return Err(arg.unexpected().into()),
        }
    }

    let id = id.ok_or_else(|| error::Error::from("missing ID argument"))?;

    Ok(Mode::Cli(Command::Todo(TodoCommand::Trace(
        TodoTraceOptions { id },
    ))))
}

pub fn trace(conf: &config::Config, opts: &TodoTraceOptions) -> error::Result<ExitCode> {
    let cwd = std::env::current_dir()?;
    let vcs_backend =
        vcs::create_vcs_backend(&cwd, &conf.get_todo_token(), None).map_err(|e| match e {
            vcs::error::Error::NotARepository => {
                error::Error::from("todo trace requires a git repository")
            }
            e => e.into(),
        })?;

    let fs_provider = FileSystemProvider::new(&conf.get_todo_token(), conf.exclude.clone());
    let todo = match fs_provider.get_todo(opts.id)? {
        Some(todo) => todo,
        None => match vcs_backend.get_todo_for_version(opts.id, "HEAD") {
            Ok(todo) => todo,
            Err(vcs::error::Error::Custom(msg)) if msg.contains("not found") => {
                return Err(format!("Todo #{} not found", opts.id).into());
            }
            Err(e) => return Err(e.into()),
        },
    };

    let history = vcs_backend.trace_todo(&todo)?;
    print_raw(&todo, &history);

    Ok(ExitCode::SUCCESS)
}

fn print_raw(todo: &Todo, history: &[(CommitMetadata, Todo)]) {
    write_raw(&mut std::io::stdout(), todo, history).unwrap();
}

fn write_raw(
    w: &mut impl std::io::Write,
    todo: &Todo,
    history: &[(CommitMetadata, Todo)],
) -> std::io::Result<()> {
    writeln!(w, "commit HEAD")?;
    writeln!(w, "    {}", todo.title)?;
    writeln!(w, "    {}", todo.location)?;

    if history.is_empty() {
        writeln!(w)?;
        writeln!(w, "No commit history found.")?;
        return Ok(());
    }

    for (commit, entry) in history {
        writeln!(w)?;
        writeln!(w, "commit {}", commit.sha)?;
        writeln!(
            w,
            "Author: {} <{}>",
            commit.author_name, commit.author_email
        )?;
        writeln!(w, "Date:   {}", commit.timestamp)?;
        writeln!(w)?;
        writeln!(w, "    {}", entry.title)?;
        writeln!(w, "    {}", entry.location)?;
    }

    Ok(())
}
