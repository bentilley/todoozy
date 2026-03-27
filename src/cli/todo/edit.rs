use crate::cli::config;
use todoozy::todo::TodoIdentifier;

pub struct TodoEditOptions {
    pub id: u32,
}

pub fn parse_opts(mut parser: lexopt::Parser) -> Result<TodoEditOptions, lexopt::Error> {
    use lexopt::prelude::*;

    let mut id: Option<u32> = None;

    while let Some(arg) = parser.next()? {
        match arg {
            Value(val) if id.is_none() => {
                id = Some(val.parse().map_err(|_| {
                    lexopt::Error::Custom(format!("invalid ID '{}'", val.to_string_lossy()).into())
                })?);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    let id = id.ok_or_else(|| lexopt::Error::Custom("missing ID argument".into()))?;

    Ok(TodoEditOptions { id })
}

pub fn edit(conf: &config::Config, opts: &TodoEditOptions) {
    let todos = match todoozy::get_todos(&conf.exclude) {
        Ok(todos) => todos,
        Err(e) => {
            eprintln!("Error loading todos: {}", e);
            std::process::exit(1);
        }
    };

    let todo = todos
        .iter()
        .find(|t| matches!(&t.id, Some(TodoIdentifier::Primary(id)) if *id == opts.id));

    let todo = match todo {
        Some(t) => t,
        None => {
            eprintln!("Todo #{} not found", opts.id);
            std::process::exit(1);
        }
    };

    let file = match &todo.file {
        Some(f) => f,
        None => {
            eprintln!("Todo #{} has no file location", opts.id);
            std::process::exit(1);
        }
    };

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

    // TODO #73 (C) 2026-03-27 Lift these exits up to src/main.rs
    //
    // Functions should end with a Result<(), Error> and let main.rs handle the exit codes. This
    // allows for better error handling, cleanup, and testing.
    match status {
        Ok(exit_status) => {
            if !exit_status.success() {
                std::process::exit(exit_status.code().unwrap_or(1));
            }
        }
        Err(e) => {
            eprintln!("Failed to launch editor '{}': {}", editor, e);
            std::process::exit(1);
        }
    }
}
