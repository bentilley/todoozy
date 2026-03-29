use super::TodoCommand;
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;

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
                id = Some(
                    val.parse()
                        .map_err(|_| error::Error::from(format!("invalid ID '{}'", val.to_string_lossy())))?,
                );
            }
            _ => return Err(arg.unexpected().into()),
        }
    }

    let id = id.ok_or_else(|| error::Error::from("missing ID argument"))?;

    Ok(Mode::Cli(Command::Todo(TodoCommand::Edit(TodoEditOptions { id }))))
}

pub fn edit(conf: &config::Config, opts: &TodoEditOptions) -> error::Result<()> {
    let todo = todoozy::get_todo(opts.id, &conf.exclude)?
        .ok_or_else(|| error::Error::from(format!("Todo #{} not found", opts.id)))?;

    let file = todo
        .file
        .as_ref()
        .ok_or_else(|| error::Error::from(format!("Todo #{} has no file location", opts.id)))?;

    let line = todo.line_number.unwrap_or(1);

    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());

    let editor_name = std::path::Path::new(&editor)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&editor);

    let status = match editor_name {
        // Vim-style editors: +<line> <file>
        "vi" | "vim" | "nvim" | "neovim" | "gvim" | "mvim" => std::process::Command::new(&editor)
            .arg(format!("+{}", line))
            .arg(file)
            .status(),
        // Emacs-style: +<line> <file>
        "emacs" | "emacsclient" => std::process::Command::new(&editor)
            .arg(format!("+{}", line))
            .arg(file)
            .status(),
        // VS Code style: --goto <file>:<line>
        "code" | "code-insiders" => std::process::Command::new(&editor)
            .arg("--goto")
            .arg(format!("{}:{}", file, line))
            .arg("--wait")
            .status(),
        // Nano: +<line> <file>
        "nano" => std::process::Command::new(&editor)
            .arg(format!("+{}", line))
            .arg(file)
            .status(),
        // Sublime Text: <file>:<line>
        "subl" | "sublime_text" => std::process::Command::new(&editor)
            .arg(format!("{}:{}", file, line))
            .arg("--wait")
            .status(),
        // Default: try +<line> syntax (works for many editors)
        _ => std::process::Command::new(&editor)
            .arg(format!("+{}", line))
            .arg(file)
            .status(),
    };

    match status {
        Ok(exit_status) => {
            if !exit_status.success() {
                return Err(format!(
                    "Editor exited with code {}",
                    exit_status.code().unwrap_or(1)
                )
                .into());
            }
        }
        Err(e) => {
            return Err(format!("Failed to launch editor '{}': {}", editor, e).into());
        }
    }

    Ok(())
}
