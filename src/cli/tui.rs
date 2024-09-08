pub mod app;
mod input;

use std::error;
use std::{io, io::stdout};

use color_eyre::config::HookBuilder;
use ratatui::{
    backend::{Backend, CrosstermBackend},
    crossterm::{
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
        ExecutableCommand,
    },
    Terminal,
};

fn init_error_hooks() -> color_eyre::Result<()> {
    let (panic, error) = HookBuilder::default().into_hooks();
    let panic = panic.into_panic_hook();
    let error = error.into_eyre_hook();
    color_eyre::eyre::set_hook(Box::new(move |e| {
        let _ = restore_terminal();
        error(e)
    }))?;
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        panic(info);
    }));
    Ok(())
}

fn init_terminal() -> io::Result<Terminal<impl Backend>> {
    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    Terminal::new(CrosstermBackend::new(stdout()))
}

fn restore_terminal() -> io::Result<()> {
    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()
}

fn get_max_todo_id(todos: &[todoozy::todo::Todo]) -> u32 {
    todos.iter().map(|t| t.id.unwrap_or(0)).max().unwrap_or(0)
}

pub fn run(mut config: crate::cli::config::Config) -> Result<(), Box<dyn error::Error>> {
    let todos = todoozy::get_todos(&config.exclude).unwrap();
    let max_id = std::cmp::max(get_max_todo_id(&todos), config.num_todos);
    if max_id > config.num_todos {
        config.num_todos = max_id;
        config.save()?;
    }
    let todos = todos
        .into_iter()
        .map(|mut t| {
            if t.id.is_none() {
                config.num_todos += 1;
                t.id = Some(config.num_todos);
            }
            t
        })
        .collect();

    init_error_hooks()?;
    let terminal = init_terminal()?;

    let mut app = app::App::new(config, todos);
    app.run(terminal)?;

    restore_terminal()?;
    Ok(())
}
